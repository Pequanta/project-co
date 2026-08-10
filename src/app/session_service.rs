use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::authorization::assert_member;
use crate::domain::{
    DomainError, MemberRole, PlanCounts, ProgressCalculator, Session, SessionStatus, User,
};
use crate::error::AppError;
use crate::eventing::EventPublisher;
use crate::repo::{NewSession, PlanRepo, ProgressRepo, SessionRepo, UserRepo};

const MAX_KEY_ATTEMPTS: usize = 5;

pub struct SessionService {
    db: PgPool,
    users: Arc<dyn UserRepo>,
    sessions: Arc<dyn SessionRepo>,
    plans: Arc<dyn PlanRepo>,
    progress: Arc<dyn ProgressRepo>,
    events: Arc<dyn EventPublisher>,
    calculator: Arc<dyn ProgressCalculator>,
}

impl SessionService {
    pub fn new(
        db: PgPool,
        users: Arc<dyn UserRepo>,
        sessions: Arc<dyn SessionRepo>,
        plans: Arc<dyn PlanRepo>,
        progress: Arc<dyn ProgressRepo>,
        events: Arc<dyn EventPublisher>,
        calculator: Arc<dyn ProgressCalculator>,
    ) -> Self {
        Self {
            db,
            users,
            sessions,
            plans,
            progress,
            events,
            calculator,
        }
    }

    /// Create a session; the creator becomes `owner`. Collision-safe key.
    pub async fn create_session(
        &self,
        actor: &User,
        project_name: String,
        project_description: Option<String>,
        deadline: DateTime<Utc>,
        initial_plans: Vec<String>,
    ) -> Result<Session, AppError> {
        let project_name = project_name.trim().to_string();
        if project_name.is_empty() {
            return Err(DomainError::EmptyProjectName.into());
        }
        if deadline <= Utc::now() {
            return Err(DomainError::InvalidDeadline.into());
        }

        let session_key = self.fresh_key().await?;

        let mut tx = self.db.begin().await?;
        let session = self
            .sessions
            .create(
                &mut tx,
                &NewSession {
                    session_key,
                    project_name,
                    project_description: project_description
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    deadline,
                    created_by: actor.id,
                },
            )
            .await?;
        self.sessions
            .add_member(&mut tx, session.id, actor.id, MemberRole::Owner)
            .await?;
        for title in initial_plans {
            let title = title.trim().to_string();
            if !title.is_empty() {
                self.plans
                    .create(&mut tx, session.id, &title, None, actor.id)
                    .await?;
            }
        }
        tx.commit().await?;

        self.events
            .publish(&crate::domain::DomainEvent::SessionCreated {
                session_id: session.id,
                actor_id: actor.id,
            })
            .await?;
        Ok(session)
    }

    /// Join by session key. Key validation, membership and session state are
    /// all enforced server-side.
    pub async fn join_session(&self, actor: &User, raw_key: &str) -> Result<Session, AppError> {
        let key = crate::domain::normalize_key(raw_key);
        if key.len() != 8 {
            return Err(DomainError::InvalidSessionKey.into());
        }

        let mut tx = self.db.begin().await?;
        let Some(session) = self.sessions.get_by_key(&mut tx, &key).await? else {
            return Err(DomainError::InvalidSessionKey.into());
        };
        if session.status != SessionStatus::Active {
            return Err(DomainError::SessionClosed.into());
        }
        if self
            .sessions
            .is_member(&mut tx, session.id, actor.id)
            .await?
        {
            return Err(DomainError::AlreadyMember.into());
        }
        self.sessions
            .add_member(&mut tx, session.id, actor.id, MemberRole::Member)
            .await?;
        tx.commit().await?;

        self.events
            .publish(&crate::domain::DomainEvent::MemberJoinedSession {
                session_id: session.id,
                actor_id: actor.id,
            })
            .await?;
        Ok(session)
    }

    pub async fn list_sessions(&self, actor_id: Uuid) -> Result<Vec<Session>, AppError> {
        let mut conn = self.db.acquire().await?;
        Ok(self.sessions.list_for_user(&mut conn, actor_id).await?)
    }

    pub async fn get_session(&self, actor_id: Uuid, session_id: Uuid) -> Result<Session, AppError> {
        let mut conn = self.db.acquire().await?;
        assert_member(&mut conn, self.sessions.as_ref(), actor_id, session_id).await?;
        self.sessions
            .get_by_id(&mut conn, session_id)
            .await?
            .ok_or(AppError::Domain(DomainError::SessionNotFound))
    }

    /// Full dashboard data for `/status`.
    pub async fn status(
        &self,
        actor_id: Uuid,
        session_id: Uuid,
    ) -> Result<StatusOverview, AppError> {
        let mut conn = self.db.acquire().await?;
        assert_member(&mut conn, self.sessions.as_ref(), actor_id, session_id).await?;

        let session = self
            .sessions
            .get_by_id(&mut conn, session_id)
            .await?
            .ok_or(AppError::Domain(DomainError::SessionNotFound))?;
        let counts = self.plans.counts_by_session(&mut conn, session_id).await?;
        let progress = self.calculator.calculate(counts);
        let members = self.sessions.members(&mut conn, session_id).await?;

        let mut member_views = Vec::with_capacity(members.len());
        for m in members {
            let user = self.users.get(&mut conn, m.user_id).await?.unwrap_or(User {
                id: m.user_id,
                telegram_user_id: 0,
                telegram_username: None,
                display_name: "Unknown".into(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
            member_views.push(MemberView { user, role: m.role });
        }

        let recent = self.progress.recent(&mut conn, session_id, 5).await?;
        let mut recent_views = Vec::with_capacity(recent.len());
        for u in recent {
            let author = self
                .users
                .get(&mut conn, u.user_id)
                .await?
                .map(|x| x.display_name)
                .unwrap_or_else(|| "Unknown".into());
            recent_views.push(RecentUpdateView {
                author_name: author,
                message: u.message,
            });
        }

        Ok(StatusOverview {
            session,
            members: member_views,
            counts,
            progress,
            recent_updates: recent_views,
        })
    }

    /// Leave a session. The owner cannot leave while other members remain.
    /// When the last member leaves, the session is archived.
    pub async fn leave_session(&self, actor_id: Uuid, session_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.db.begin().await?;
        if !self
            .sessions
            .is_member(&mut tx, session_id, actor_id)
            .await?
        {
            return Err(DomainError::NotMember.into());
        }
        let members = self.sessions.members(&mut tx, session_id).await?;
        let mine = members
            .iter()
            .find(|m| m.user_id == actor_id)
            .ok_or(DomainError::NotMember)?;
        if mine.role == MemberRole::Owner && members.len() > 1 {
            return Err(DomainError::OwnerCannotLeave.into());
        }
        self.sessions
            .remove_member(&mut tx, session_id, actor_id)
            .await?;
        if self.sessions.count_members(&mut tx, session_id).await? == 0 {
            self.sessions
                .set_status(&mut tx, session_id, SessionStatus::Archived)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn fresh_key(&self) -> Result<String, AppError> {
        let mut conn = self.db.acquire().await?;
        for _ in 0..MAX_KEY_ATTEMPTS {
            let key = crate::domain::generate_session_key();
            if self.sessions.get_by_key(&mut conn, &key).await?.is_none() {
                return Ok(key);
            }
        }
        Err(DomainError::KeyCollision.into())
    }
}

pub struct StatusOverview {
    pub session: Session,
    pub members: Vec<MemberView>,
    pub counts: PlanCounts,
    pub progress: u8,
    pub recent_updates: Vec<RecentUpdateView>,
}

pub struct MemberView {
    pub user: User,
    pub role: MemberRole,
}

pub struct RecentUpdateView {
    pub author_name: String,
    pub message: String,
}
