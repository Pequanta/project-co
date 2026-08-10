use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Completed,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum PlanStatus {
    Planned,
    InProgress,
    Completed,
    Cancelled,
}

impl PlanStatus {
    pub fn name(&self) -> &'static str {
        match self {
            PlanStatus::Planned => "planned",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Completed => "completed",
            PlanStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, PlanStatus::Planned | PlanStatus::InProgress)
    }

    /// planned -> in_progress
    pub fn can_start(&self) -> bool {
        matches!(self, PlanStatus::Planned)
    }

    /// planned | in_progress -> completed
    pub fn can_complete(&self) -> bool {
        matches!(self, PlanStatus::Planned | PlanStatus::InProgress)
    }

    /// planned | in_progress -> cancelled
    pub fn can_cancel(&self) -> bool {
        matches!(self, PlanStatus::Planned | PlanStatus::InProgress)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub telegram_user_id: i64,
    pub telegram_username: Option<String>,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: Uuid,
    pub session_key: String,
    pub project_name: String,
    pub project_description: Option<String>,
    pub deadline: DateTime<Utc>,
    pub created_by: Uuid,
    pub status: SessionStatus,
    pub last_reminder_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionMember {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Plan {
    pub id: Uuid,
    pub session_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: PlanStatus,
    pub created_by: Uuid,
    pub assigned_to: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProgressUpdate {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub message: String,
    pub created_at: DateTime<Utc>,
}
