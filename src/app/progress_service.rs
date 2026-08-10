use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::app::authorization::assert_member;
use crate::domain::{DomainError, DomainEvent, ProgressUpdate};
use crate::error::AppError;
use crate::eventing::EventPublisher;
use crate::repo::{ProgressRepo, SessionRepo};

/// Append-only progress updates; history is never overwritten.
pub struct ProgressService {
    db: PgPool,
    sessions: Arc<dyn SessionRepo>,
    progress: Arc<dyn ProgressRepo>,
    events: Arc<dyn EventPublisher>,
}

impl ProgressService {
    pub fn new(
        db: PgPool,
        sessions: Arc<dyn SessionRepo>,
        progress: Arc<dyn ProgressRepo>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            db,
            sessions,
            progress,
            events,
        }
    }

    pub async fn submit(
        &self,
        actor_id: Uuid,
        session_id: Uuid,
        message: &str,
    ) -> Result<ProgressUpdate, AppError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(DomainError::EmptyProgress.into());
        }
        let mut tx = self.db.begin().await?;
        assert_member(&mut tx, self.sessions.as_ref(), actor_id, session_id).await?;
        let update = self
            .progress
            .insert(&mut tx, session_id, actor_id, message)
            .await?;
        tx.commit().await?;

        self.events
            .publish(&DomainEvent::ProgressUpdated {
                session_id,
                user_id: actor_id,
                update_id: update.id,
            })
            .await?;
        Ok(update)
    }
}
