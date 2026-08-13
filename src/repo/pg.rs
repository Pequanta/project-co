//! Postgres implementations of the repository traits.
//!
//! All methods take a `&mut PgConnection`, so callers can pass either an
//! acquired connection or an open transaction (which derefs to a connection)
//! and compose transactional units themselves.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::domain::{
    DomainEvent, MemberRole, Plan, PlanCounts, PlanStatus, ProgressUpdate, Session, SessionMember,
    SessionStatus, User,
};
use crate::repo::{
    BroadcastTarget, Conversation, ConversationRepo, DedupeRepo, EventRepo, NewSession,
    NotificationRepo, PlanCompletionRepo, PlanRepo, ProgressRepo, SessionBroadcastRepo,
    SessionRepo, UserRepo,
};

pub struct PgUserRepo;

#[async_trait]
impl UserRepo for PgUserRepo {
    async fn upsert(
        &self,
        exec: &mut PgConnection,
        telegram_user_id: i64,
        username: Option<&str>,
        display_name: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"INSERT INTO users (telegram_user_id, telegram_username, display_name)
               VALUES ($1, $2, $3)
               ON CONFLICT (telegram_user_id) DO UPDATE SET
                 telegram_username = EXCLUDED.telegram_username,
                 display_name = EXCLUDED.display_name,
                 updated_at = now()
               RETURNING id, telegram_user_id, telegram_username, display_name, created_at, updated_at"#,
        )
        .bind(telegram_user_id)
        .bind(username)
        .bind(display_name)
        .fetch_one(exec)
        .await
    }

    async fn get(&self, exec: &mut PgConnection, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, telegram_user_id, telegram_username, display_name, created_at, updated_at \
             FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(exec)
        .await
    }
}

pub struct PgSessionRepo;

#[async_trait]
impl SessionRepo for PgSessionRepo {
    async fn create(
        &self,
        exec: &mut PgConnection,
        new_session: &NewSession,
    ) -> Result<Session, sqlx::Error> {
        sqlx::query_as::<_, Session>(
            r#"INSERT INTO sessions
                 (session_key, project_name, project_description, deadline, created_by, mode)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, session_key, project_name, project_description, deadline,
                         created_by, status, mode, last_reminder_at, created_at, updated_at"#,
        )
        .bind(&new_session.session_key)
        .bind(&new_session.project_name)
        .bind(new_session.project_description.as_deref())
        .bind(new_session.deadline)
        .bind(new_session.created_by)
        .bind(new_session.mode)
        .fetch_one(exec)
        .await
    }

    async fn get_by_id(
        &self,
        exec: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>(
            "SELECT id, session_key, project_name, project_description, deadline, created_by, \
                    status, mode, last_reminder_at, created_at, updated_at \
             FROM sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(exec)
        .await
    }

    async fn get_by_key(
        &self,
        exec: &mut PgConnection,
        key: &str,
    ) -> Result<Option<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>(
            "SELECT id, session_key, project_name, project_description, deadline, created_by, \
                    status, mode, last_reminder_at, created_at, updated_at \
             FROM sessions WHERE session_key = $1",
        )
        .bind(key)
        .fetch_optional(exec)
        .await
    }

    async fn list_for_user(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
    ) -> Result<Vec<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>(
            r#"SELECT s.id, s.session_key, s.project_name, s.project_description, s.deadline,
                      s.created_by, s.status, s.mode, s.last_reminder_at, s.created_at, s.updated_at
               FROM sessions s
               JOIN session_members m ON m.session_id = s.id
               WHERE m.user_id = $1
               ORDER BY s.created_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(exec)
        .await
    }

    async fn due_for_reminder(
        &self,
        exec: &mut PgConnection,
        window_hours: i64,
    ) -> Result<Vec<Session>, sqlx::Error> {
        sqlx::query_as::<_, Session>(
            r#"SELECT id, session_key, project_name, project_description, deadline, created_by,
                      status, mode, last_reminder_at, created_at, updated_at
               FROM sessions
               WHERE status = 'active'
                 AND (last_reminder_at IS NULL
                      OR last_reminder_at < now() - interval '24 hours')
                 AND (deadline <= now()
                      OR deadline < now() + make_interval(hours => $1))
               ORDER BY deadline ASC"#,
        )
        .bind(window_hours)
        .fetch_all(exec)
        .await
    }

    async fn mark_reminded(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE sessions SET last_reminder_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(session_id)
        .execute(exec)
        .await
        .map(|_| ())
    }

    async fn set_status(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        status: SessionStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE sessions SET status = $2, updated_at = now() WHERE id = $1")
            .bind(session_id)
            .bind(status)
            .execute(exec)
            .await
            .map(|_| ())
    }

    async fn is_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM session_members WHERE session_id = $1 AND user_id = $2)",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(exec)
        .await
    }

    async fn members(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<SessionMember>, sqlx::Error> {
        sqlx::query_as::<_, SessionMember>(
            "SELECT id, session_id, user_id, role, joined_at FROM session_members \
             WHERE session_id = $1 ORDER BY joined_at ASC",
        )
        .bind(session_id)
        .fetch_all(exec)
        .await
    }

    async fn add_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
        role: MemberRole,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO session_members (session_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(session_id)
            .bind(user_id)
            .bind(role)
            .execute(exec)
            .await
            .map(|_| ())
    }

    async fn remove_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM session_members WHERE session_id = $1 AND user_id = $2")
            .bind(session_id)
            .bind(user_id)
            .execute(exec)
            .await
            .map(|_| ())
    }

    async fn count_members(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM session_members WHERE session_id = $1")
            .bind(session_id)
            .fetch_one(exec)
            .await
    }
}

pub struct PgPlanRepo;

#[async_trait]
impl PlanRepo for PgPlanRepo {
    async fn create(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        title: &str,
        description: Option<&str>,
        created_by: Uuid,
    ) -> Result<Plan, sqlx::Error> {
        sqlx::query_as::<_, Plan>(
            r#"INSERT INTO plans (session_id, title, description, created_by)
               VALUES ($1, $2, $3, $4)
               RETURNING id, session_id, title, description, status, created_by, assigned_to,
                         created_at, completed_at, updated_at"#,
        )
        .bind(session_id)
        .bind(title)
        .bind(description)
        .bind(created_by)
        .fetch_one(exec)
        .await
    }

    async fn get(
        &self,
        exec: &mut PgConnection,
        plan_id: Uuid,
    ) -> Result<Option<Plan>, sqlx::Error> {
        sqlx::query_as::<_, Plan>(
            "SELECT id, session_id, title, description, status, created_by, assigned_to, \
                    created_at, completed_at, updated_at \
             FROM plans WHERE id = $1",
        )
        .bind(plan_id)
        .fetch_optional(exec)
        .await
    }

    async fn list_by_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<Plan>, sqlx::Error> {
        sqlx::query_as::<_, Plan>(
            "SELECT id, session_id, title, description, status, created_by, assigned_to, \
                    created_at, completed_at, updated_at \
             FROM plans WHERE session_id = $1 ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(exec)
        .await
    }

    async fn set_status(
        &self,
        exec: &mut PgConnection,
        plan_id: Uuid,
        status: PlanStatus,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE plans SET status = $2, completed_at = $3, updated_at = now() WHERE id = $1",
        )
        .bind(plan_id)
        .bind(status)
        .bind(completed_at)
        .execute(exec)
        .await
        .map(|_| ())
    }

    async fn counts_by_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<PlanCounts, sqlx::Error> {
        sqlx::query_as::<_, PlanCounts>(
            r#"SELECT
                 count(*) FILTER (WHERE status = 'completed')  AS completed,
                 count(*) FILTER (WHERE status = 'in_progress') AS in_progress,
                 count(*) FILTER (WHERE status = 'planned')     AS planned,
                 count(*) FILTER (WHERE status = 'cancelled')   AS cancelled
               FROM plans WHERE session_id = $1"#,
        )
        .bind(session_id)
        .fetch_one(exec)
        .await
    }
}

pub struct PgPlanCompletionRepo;

#[async_trait]
impl PlanCompletionRepo for PgPlanCompletionRepo {
    async fn insert(
        &self,
        exec: &mut PgConnection,
        plan_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            "INSERT INTO plan_completions (plan_id, user_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(plan_id)
        .bind(user_id)
        .execute(exec)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn completed_counts_by_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, i64)>(
            r#"SELECT pc.user_id, count(*) AS completed
               FROM plan_completions pc
               JOIN plans p ON p.id = pc.plan_id
               WHERE p.session_id = $1
               GROUP BY pc.user_id"#,
        )
        .bind(session_id)
        .fetch_all(exec)
        .await
    }

    async fn completed_plan_ids_for_member(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT pc.plan_id
               FROM plan_completions pc
               JOIN plans p ON p.id = pc.plan_id
               WHERE p.session_id = $1 AND pc.user_id = $2"#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_all(exec)
        .await
    }
}

pub struct PgProgressRepo;

#[async_trait]
impl ProgressRepo for PgProgressRepo {
    async fn insert(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        user_id: Uuid,
        message: &str,
    ) -> Result<ProgressUpdate, sqlx::Error> {
        sqlx::query_as::<_, ProgressUpdate>(
            "INSERT INTO progress_updates (session_id, user_id, message) VALUES ($1, $2, $3) \
             RETURNING id, session_id, user_id, message, created_at",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(message)
        .fetch_one(exec)
        .await
    }

    async fn get(
        &self,
        exec: &mut PgConnection,
        update_id: Uuid,
    ) -> Result<Option<ProgressUpdate>, sqlx::Error> {
        sqlx::query_as::<_, ProgressUpdate>(
            "SELECT id, session_id, user_id, message, created_at FROM progress_updates \
             WHERE id = $1",
        )
        .bind(update_id)
        .fetch_optional(exec)
        .await
    }

    async fn recent(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ProgressUpdate>, sqlx::Error> {
        sqlx::query_as::<_, ProgressUpdate>(
            "SELECT id, session_id, user_id, message, created_at FROM progress_updates \
             WHERE session_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(exec)
        .await
    }
}

pub struct PgEventRepo;

#[async_trait]
impl EventRepo for PgEventRepo {
    async fn push(&self, exec: &mut PgConnection, event: &DomainEvent) -> Result<i64, sqlx::Error> {
        let payload = serde_json::to_value(event).map_err(|e| sqlx::Error::Encode(e.into()))?;
        sqlx::query_as::<_, (i64,)>(
            r#"INSERT INTO events (event_type, session_id, actor_id, payload)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
        )
        .bind(event.name())
        .bind(event.session_id())
        .bind(event.actor_id())
        .bind(payload)
        .fetch_one(exec)
        .await
        .map(|(id,)| id)
    }
}

pub struct PgNotificationRepo;

#[async_trait]
impl NotificationRepo for PgNotificationRepo {
    async fn create_pending(
        &self,
        exec: &mut PgConnection,
        event_id: i64,
        session_id: Uuid,
        recipient_telegram_id: i64,
        message: &str,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_as::<_, (i64,)>(
            r#"INSERT INTO notifications (event_id, session_id, recipient_telegram_id, message)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
        )
        .bind(event_id)
        .bind(session_id)
        .bind(recipient_telegram_id)
        .bind(message)
        .fetch_one(exec)
        .await
        .map(|(id,)| id)
    }

    async fn mark_result(
        &self,
        exec: &mut PgConnection,
        notification_id: i64,
        sent: bool,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE notifications
               SET status = $2, error = $3, attempts = attempts + 1,
                   sent_at = CASE WHEN $2 = 'sent' THEN now() ELSE sent_at END
               WHERE id = $1"#,
        )
        .bind(notification_id)
        .bind(if sent { "sent" } else { "failed" })
        .bind(error)
        .execute(exec)
        .await
        .map(|_| ())
    }
}

pub struct PgSessionBroadcastRepo;

#[async_trait]
impl SessionBroadcastRepo for PgSessionBroadcastRepo {
    async fn add(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        chat_id: i64,
        chat_type: &str,
        title: Option<&str>,
        linked_by: Option<Uuid>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"INSERT INTO session_broadcast_targets
                 (session_id, chat_id, chat_type, title, linked_by)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (session_id, chat_id) DO NOTHING"#,
        )
        .bind(session_id)
        .bind(chat_id)
        .bind(chat_type)
        .bind(title)
        .bind(linked_by)
        .execute(exec)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn list_for_session(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<Vec<BroadcastTarget>, sqlx::Error> {
        #[derive(FromRow)]
        struct Row {
            session_id: Uuid,
            chat_id: i64,
            chat_type: String,
            title: Option<String>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT session_id, chat_id, chat_type, title FROM session_broadcast_targets \
             WHERE session_id = $1 ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(exec)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| BroadcastTarget {
                session_id: r.session_id,
                chat_id: r.chat_id,
                chat_type: r.chat_type,
                title: r.title,
            })
            .collect())
    }

    async fn remove(
        &self,
        exec: &mut PgConnection,
        session_id: Uuid,
        chat_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            "DELETE FROM session_broadcast_targets WHERE session_id = $1 AND chat_id = $2",
        )
        .bind(session_id)
        .bind(chat_id)
        .execute(exec)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn remove_all_for_chat(
        &self,
        exec: &mut PgConnection,
        chat_id: i64,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            "DELETE FROM session_broadcast_targets WHERE chat_id = $1 RETURNING session_id",
        )
        .bind(chat_id)
        .fetch_all(exec)
        .await
    }
}

pub struct PgConversationRepo;

#[async_trait]
impl ConversationRepo for PgConversationRepo {
    async fn get(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
    ) -> Result<Option<Conversation>, sqlx::Error> {
        #[derive(FromRow)]
        struct Row {
            user_id: Uuid,
            chat_id: i64,
            state: String,
            payload: serde_json::Value,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT user_id, chat_id, state, payload FROM bot_conversations WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(exec)
        .await?;
        Ok(row.map(|r| Conversation {
            user_id: r.user_id,
            chat_id: r.chat_id,
            state: r.state,
            payload: r.payload,
        }))
    }

    async fn set(
        &self,
        exec: &mut PgConnection,
        user_id: Uuid,
        chat_id: i64,
        state: &str,
        payload: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO bot_conversations (user_id, chat_id, state, payload)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (user_id) DO UPDATE SET
                 chat_id = EXCLUDED.chat_id,
                 state = EXCLUDED.state,
                 payload = EXCLUDED.payload,
                 updated_at = now()"#,
        )
        .bind(user_id)
        .bind(chat_id)
        .bind(state)
        .bind(payload)
        .execute(exec)
        .await
        .map(|_| ())
    }

    async fn clear(&self, exec: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM bot_conversations WHERE user_id = $1")
            .bind(user_id)
            .execute(exec)
            .await
            .map(|_| ())
    }
}

pub struct PgDedupeRepo;

#[async_trait]
impl DedupeRepo for PgDedupeRepo {
    async fn mark_seen(
        &self,
        exec: &mut PgConnection,
        update_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> = sqlx::query_as(
            "INSERT INTO processed_updates (update_id) VALUES ($1) \
             ON CONFLICT (update_id) DO NOTHING RETURNING update_id",
        )
        .bind(update_id)
        .fetch_optional(exec)
        .await?;
        Ok(row.is_some())
    }
}
