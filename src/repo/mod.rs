//! Persistence layer: repository traits. Implementations (Postgres) live in
//! `repo::pg`. Services depend on these traits so the storage backend is
//! replaceable. All session-scoped reads/writes go through authorization
//! checks at the application layer — see `app::authorization`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::{
    DomainEvent, MemberRole, Plan, PlanCounts, ProgressUpdate, Session, SessionMember,
    SessionStatus, User,
};

pub mod pg;

#[async_trait]
pub trait UserRepo: Send + Sync {
    /// Create the user, or update display info if they already exist.
    async fn upsert(
        &self,
        exec: &mut PgConnection,
        telegram_user_id: i64,
        username: Option<&str>,
        display_name: &str,
    ) -> Result<User, sqlx::Error>;

    async fn get(&self, exec: &mut PgConnection, id: Uuid) -> Result<Option<User>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub session_key: String,
    pub project_name: String,
    pub project_description: Option<String>,
    pub deadline: DateTime<Utc>,
    pub created_by: Uuid,
}

#[async_trait]
pub trait SessionRepo: Send + Sync {
    async fn create(
        &self,
        exec: &mut PgConnection,
        new_session: &NewSession,
    ) -> Result<Session, sqlx::Error>;

    async fn get_by_id(
        &self,
        exec: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<Session>, sqlx::Error>;

    async fn get_by_key(
        &self,
        exec: &mut PgConnection,
        key: &str,
    ) -> Result<Option<Session>, sqlx::Error>;

    /// Sessions the user is a member of (all statuses).
    async fn list_for_user(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
    ) -> Result<Vec<Session>, sqlx::Error>;

    /// Sessions that need a deadline reminder: active, deadline passed or
    /// within `window_hours`, and not reminded in the last 24h.
    async fn due_for_reminder(
        &self,
        exec: &mut PgConnection,
        window_hours: i64,
    ) -> Result<Vec<Session>, sqlx::Error>;

    async fn mark_reminded(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    async fn set_status(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        status: SessionStatus,
    ) -> Result<(), sqlx::Error>;

    async fn is_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error>;

    async fn members(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<SessionMember>, sqlx::Error>;

    async fn add_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
        role: MemberRole,
    ) -> Result<(), sqlx::Error>;

    async fn remove_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    async fn count_members(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<i64, sqlx::Error>;
}

#[async_trait]
pub trait PlanRepo: Send + Sync {
    async fn create(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        title: &str,
        description: Option<&str>,
        created_by: Uuid,
    ) -> Result<Plan, sqlx::Error>;

    async fn get(
        &self,
        exec: &mut PgConnection,
        plan_id: Uuid,
    ) -> Result<Option<Plan>, sqlx::Error>;

    async fn list_by_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<Plan>, sqlx::Error>;

    async fn set_status(
        &self,
        exec: &mut PgConnection,
        plan_id: Uuid,
        status: crate::domain::PlanStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error>;

    async fn counts_by_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<PlanCounts, sqlx::Error>;
}

#[async_trait]
pub trait ProgressRepo: Send + Sync {
    async fn insert(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
        message: &str,
    ) -> Result<ProgressUpdate, sqlx::Error>;

    async fn get(
        &self,
        exec: &mut PgConnection,
        update_id: Uuid,
    ) -> Result<Option<ProgressUpdate>, sqlx::Error>;

    async fn recent(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ProgressUpdate>, sqlx::Error>;
}

#[async_trait]
pub trait EventRepo: Send + Sync {
    /// Persist an event to the outbox, returning its id.
    async fn push(&self, exec: &mut PgConnection, event: &DomainEvent) -> Result<i64, sqlx::Error>;
}

#[async_trait]
pub trait NotificationRepo: Send + Sync {
    async fn create_pending(
        &self,
        exec: &mut PgConnection,
        event_id: i64,
        session_id: Uuid,
        recipient_telegram_id: i64,
        message: &str,
    ) -> Result<i64, sqlx::Error>;

    async fn mark_result(
        &self,
        exec: &mut PgConnection,
        notification_id: i64,
        sent: bool,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error>;
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Conversation {
    pub user_id: Uuid,
    pub chat_id: i64,
    pub state: String,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait ConversationRepo: Send + Sync {
    async fn get(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
    ) -> Result<Option<Conversation>, sqlx::Error>;

    async fn set(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
        chat_id: i64,
        state: &str,
        payload: serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    #[allow(dead_code)]
    async fn clear(&self, exec: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait DedupeRepo: Send + Sync {
    /// Record an update_id. Returns `false` if it was already processed.
    async fn mark_seen(&self, exec: &mut PgConnection, update_id: i64)
        -> Result<bool, sqlx::Error>;
}
