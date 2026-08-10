use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::authorization::assert_member;
use crate::domain::{DomainError, DomainEvent, Plan, PlanStatus};
use crate::error::AppError;
use crate::eventing::EventPublisher;
use crate::repo::{PlanRepo, SessionRepo};

/// Lifecycle operations for plans. Every operation re-validates membership and
/// plan state transitions server-side.
pub struct PlanService {
    db: PgPool,
    sessions: Arc<dyn SessionRepo>,
    plans: Arc<dyn PlanRepo>,
    events: Arc<dyn EventPublisher>,
}

impl PlanService {
    pub fn new(
        db: PgPool,
        sessions: Arc<dyn SessionRepo>,
        plans: Arc<dyn PlanRepo>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            db,
            sessions,
            plans,
            events,
        }
    }

    pub async fn create_plan(
        &self,
        actor_id: Uuid,
        session_id: Uuid,
        title: &str,
        description: Option<&str>,
    ) -> Result<Plan, AppError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(DomainError::EmptyTitle.into());
        }
        let mut tx = self.db.begin().await?;
        assert_member(&mut tx, self.sessions.as_ref(), actor_id, session_id).await?;
        let plan = self
            .plans
            .create(
                &mut tx,
                session_id,
                &title,
                description.map(str::trim),
                actor_id,
            )
            .await?;
        tx.commit().await?;

        self.events
            .publish(&DomainEvent::PlanCreated {
                session_id,
                plan_id: plan.id,
                actor_id,
            })
            .await?;
        Ok(plan)
    }

    pub async fn list_plans(
        &self,
        actor_id: Uuid,
        session_id: Uuid,
    ) -> Result<Vec<Plan>, AppError> {
        let mut conn = self.db.acquire().await?;
        assert_member(&mut conn, self.sessions.as_ref(), actor_id, session_id).await?;
        Ok(self.plans.list_by_session(&mut conn, session_id).await?)
    }

    /// planned -> in_progress
    #[allow(dead_code)]
    pub async fn start_plan(&self, actor_id: Uuid, plan_id: Uuid) -> Result<Plan, AppError> {
        self.transition(actor_id, plan_id, PlanStatus::InProgress, None)
            .await
    }

    /// -> completed. Idempotent: completing an already-completed plan is a no-op.
    pub async fn complete_plan(&self, actor_id: Uuid, plan_id: Uuid) -> Result<Plan, AppError> {
        self.transition(actor_id, plan_id, PlanStatus::Completed, Some(Utc::now()))
            .await
    }

    /// -> cancelled. Idempotent.
    #[allow(dead_code)]
    pub async fn cancel_plan(&self, actor_id: Uuid, plan_id: Uuid) -> Result<Plan, AppError> {
        self.transition(actor_id, plan_id, PlanStatus::Cancelled, None)
            .await
    }

    async fn transition(
        &self,
        actor_id: Uuid,
        plan_id: Uuid,
        target: PlanStatus,
        completed_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Plan, AppError> {
        let mut tx = self.db.begin().await?;
        let plan = self
            .plans
            .get(&mut tx, plan_id)
            .await?
            .ok_or(DomainError::PlanNotFound)?;

        // Idempotency: asking to reach a state you're already in is a success.
        if plan.status == target {
            tx.commit().await?;
            return Ok(plan);
        }

        let allowed = match target {
            PlanStatus::Completed => plan.status.can_complete(),
            PlanStatus::InProgress => plan.status.can_start(),
            PlanStatus::Cancelled => plan.status.can_cancel(),
            PlanStatus::Planned => false,
        };
        if !allowed {
            return Err(DomainError::InvalidTransition(format!(
                "{} -> {}",
                plan.status.name(),
                target.name()
            ))
            .into());
        }

        assert_member(&mut tx, self.sessions.as_ref(), actor_id, plan.session_id).await?;
        self.plans
            .set_status(&mut tx, plan_id, target, completed_at)
            .await?;
        tx.commit().await?;

        let event = match target {
            PlanStatus::InProgress => DomainEvent::PlanStarted {
                session_id: plan.session_id,
                plan_id,
                actor_id,
            },
            PlanStatus::Completed => DomainEvent::PlanCompleted {
                session_id: plan.session_id,
                plan_id,
                actor_id,
            },
            PlanStatus::Cancelled => DomainEvent::PlanCancelled {
                session_id: plan.session_id,
                plan_id,
                actor_id,
            },
            PlanStatus::Planned => unreachable!(),
        };
        self.events.publish(&event).await?;
        Ok(plan)
    }
}
