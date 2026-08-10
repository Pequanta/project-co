use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;

use crate::domain::{
    study_overall_percent, DomainEvent, ProgressCalculator, SessionMember, SessionMode,
};
use crate::error::AppError;
use crate::notify::messages::{event_text, NotifyCtx};
use crate::notify::Notifier;
use crate::repo::{
    NotificationRepo, PlanCompletionRepo, PlanRepo, ProgressRepo, SessionRepo, UserRepo,
};
use crate::telegram::gateway::TelegramGateway;
use crate::text;

/// Consumes events and broadcasts to all session members except the actor.
/// Each recipient gets a row in `notifications`; delivery outcome is recorded
/// and failures are logged, never fatal.
pub struct NotificationService {
    db: PgPool,
    users: Arc<dyn UserRepo>,
    sessions: Arc<dyn SessionRepo>,
    plans: Arc<dyn PlanRepo>,
    completions: Arc<dyn PlanCompletionRepo>,
    progress: Arc<dyn ProgressRepo>,
    notifications: Arc<dyn NotificationRepo>,
    gateway: Arc<dyn TelegramGateway>,
    calculator: Arc<dyn ProgressCalculator>,
}

impl NotificationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: PgPool,
        users: Arc<dyn UserRepo>,
        sessions: Arc<dyn SessionRepo>,
        plans: Arc<dyn PlanRepo>,
        completions: Arc<dyn PlanCompletionRepo>,
        progress: Arc<dyn ProgressRepo>,
        notifications: Arc<dyn NotificationRepo>,
        gateway: Arc<dyn TelegramGateway>,
        calculator: Arc<dyn ProgressCalculator>,
    ) -> Self {
        Self {
            db,
            users,
            sessions,
            plans,
            completions,
            progress,
            notifications,
            gateway,
            calculator,
        }
    }
}

#[async_trait::async_trait]
impl Notifier for NotificationService {
    async fn handle_event(
        &self,
        event: &DomainEvent,
        event_id: Option<i64>,
    ) -> Result<(), AppError> {
        let Some(session_id) = event.session_id() else {
            return Ok(());
        };
        let actor_id = event.actor_id();

        let mut conn = self.db.acquire().await?;
        let Some(session) = self.sessions.get_by_id(&mut conn, session_id).await? else {
            return Ok(()); // session gone (e.g. archived), nothing to notify
        };
        let members = self.sessions.members(&mut conn, session_id).await?;
        let recipients: Vec<&SessionMember> = members
            .iter()
            .filter(|m| Some(m.user_id) != actor_id)
            .collect();
        if recipients.is_empty() {
            return Ok(()); // nobody to broadcast to
        }

        let actor_name = match actor_id {
            Some(id) => self.users.get(&mut conn, id).await?.map(|u| u.display_name),
            None => None,
        };
        let plan_title = match event.plan_id() {
            Some(pid) => self.plans.get(&mut conn, pid).await?.map(|p| p.title),
            None => None,
        };
        let update_message = match event {
            DomainEvent::ProgressUpdated { update_id, .. } => self
                .progress
                .get(&mut conn, *update_id)
                .await?
                .map(|u| u.message),
            _ => None,
        };
        let counts = self.plans.counts_by_session(&mut conn, session_id).await?;
        let progress_pct = match session.mode {
            SessionMode::Collaboration => self.calculator.calculate(counts),
            SessionMode::Study => {
                let per_member: Vec<i64> = self
                    .completions
                    .completed_counts_by_session(&mut conn, session_id)
                    .await?
                    .into_iter()
                    .map(|(_, c)| c)
                    .collect();
                // Members with zero completions are absent from the grouped
                // counts, so pad to the full member roster for a correct avg.
                let mut counts_vec = per_member;
                counts_vec.resize(members.len(), 0);
                study_overall_percent(&counts_vec, counts.total_active())
            }
        };

        let ctx = NotifyCtx {
            event,
            session: &session,
            actor_name,
            plan_title,
            progress: progress_pct,
            remaining_active: counts.in_progress + counts.planned,
            days_to_deadline: text::days_remaining(session.deadline, Utc::now()),
            update_message,
        };
        let message = event_text(&ctx);
        if message.is_empty() {
            return Ok(());
        }

        for member in recipients {
            let Some(tg_id) = self
                .users
                .get(&mut conn, member.user_id)
                .await?
                .map(|u| u.telegram_user_id)
            else {
                continue;
            };

            let notification_id = self
                .notifications
                .create_pending(
                    &mut conn,
                    event_id.unwrap_or(0),
                    session_id,
                    tg_id,
                    &message,
                )
                .await?;

            match self.gateway.send_message(tg_id, &message, None).await {
                Ok(_) => {
                    self.notifications
                        .mark_result(&mut conn, notification_id, true, None)
                        .await?;
                }
                Err(e) => {
                    tracing::warn!(
                        tg_id,
                        event = event.name(),
                        error = %e,
                        "notification delivery failed (blocked bot?)"
                    );
                    self.notifications
                        .mark_result(&mut conn, notification_id, false, Some(&e.to_string()))
                        .await?;
                }
            }
        }
        Ok(())
    }
}
