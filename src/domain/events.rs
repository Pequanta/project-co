use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Domain events. The notification subsystem consumes these; the backend/db is
/// the source of truth, never Telegram. Persisted in the `events` outbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    SessionCreated {
        session_id: Uuid,
        actor_id: Uuid,
    },
    MemberJoinedSession {
        session_id: Uuid,
        actor_id: Uuid,
    },
    PlanCreated {
        session_id: Uuid,
        plan_id: Uuid,
        actor_id: Uuid,
    },
    PlanStarted {
        session_id: Uuid,
        plan_id: Uuid,
        actor_id: Uuid,
    },
    PlanCompleted {
        session_id: Uuid,
        plan_id: Uuid,
        actor_id: Uuid,
    },
    PlanCancelled {
        session_id: Uuid,
        plan_id: Uuid,
        actor_id: Uuid,
    },
    ProgressUpdated {
        session_id: Uuid,
        user_id: Uuid,
        update_id: Uuid,
    },
    DeadlineApproaching {
        session_id: Uuid,
    },
    DeadlineReached {
        session_id: Uuid,
    },
}

impl DomainEvent {
    pub fn name(&self) -> &'static str {
        match self {
            DomainEvent::SessionCreated { .. } => "session_created",
            DomainEvent::MemberJoinedSession { .. } => "member_joined_session",
            DomainEvent::PlanCreated { .. } => "plan_created",
            DomainEvent::PlanStarted { .. } => "plan_started",
            DomainEvent::PlanCompleted { .. } => "plan_completed",
            DomainEvent::PlanCancelled { .. } => "plan_cancelled",
            DomainEvent::ProgressUpdated { .. } => "progress_updated",
            DomainEvent::DeadlineApproaching { .. } => "deadline_approaching",
            DomainEvent::DeadlineReached { .. } => "deadline_reached",
        }
    }

    pub fn session_id(&self) -> Option<Uuid> {
        match self {
            DomainEvent::SessionCreated { session_id, .. }
            | DomainEvent::MemberJoinedSession { session_id, .. }
            | DomainEvent::PlanCreated { session_id, .. }
            | DomainEvent::PlanStarted { session_id, .. }
            | DomainEvent::PlanCompleted { session_id, .. }
            | DomainEvent::PlanCancelled { session_id, .. }
            | DomainEvent::ProgressUpdated { session_id, .. }
            | DomainEvent::DeadlineApproaching { session_id }
            | DomainEvent::DeadlineReached { session_id } => Some(*session_id),
        }
    }

    /// The user who caused the event, if any. Notifications are not sent back
    /// to the actor (they already saw the result in their own chat).
    pub fn actor_id(&self) -> Option<Uuid> {
        match self {
            DomainEvent::SessionCreated { actor_id, .. }
            | DomainEvent::MemberJoinedSession { actor_id, .. }
            | DomainEvent::PlanCreated { actor_id, .. }
            | DomainEvent::PlanStarted { actor_id, .. }
            | DomainEvent::PlanCompleted { actor_id, .. }
            | DomainEvent::PlanCancelled { actor_id, .. } => Some(*actor_id),
            DomainEvent::ProgressUpdated { user_id, .. } => Some(*user_id),
            _ => None,
        }
    }

    pub fn plan_id(&self) -> Option<Uuid> {
        match self {
            DomainEvent::PlanCreated { plan_id, .. }
            | DomainEvent::PlanStarted { plan_id, .. }
            | DomainEvent::PlanCompleted { plan_id, .. }
            | DomainEvent::PlanCancelled { plan_id, .. } => Some(*plan_id),
            _ => None,
        }
    }
}
