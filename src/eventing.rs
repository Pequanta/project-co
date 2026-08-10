//! Event system: business events drive notifications. Publishing persists to
//! the `events` outbox first (backend is the source of truth), then hands the
//! event to the notification subsystem for delivery. Delivery is best-effort:
//! failures are recorded, never fatal to the originating operation.

use async_trait::async_trait;
use std::sync::Arc;

use sqlx::PgPool;

use crate::domain::DomainEvent;
use crate::error::AppError;
use crate::notify::Notifier;
use crate::repo::{pg::PgEventRepo, EventRepo};

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Persist the event to the outbox and dispatch it for notification.
    async fn publish(&self, event: &DomainEvent) -> Result<(), AppError>;
}

pub struct PgEventPublisher {
    db: PgPool,
    events: Arc<dyn EventRepo>,
    notifier: Arc<dyn Notifier>,
}

impl PgEventPublisher {
    pub fn new(db: PgPool, notifier: Arc<dyn Notifier>) -> Self {
        Self {
            db,
            events: Arc::new(PgEventRepo),
            notifier,
        }
    }
}

#[async_trait]
impl EventPublisher for PgEventPublisher {
    async fn publish(&self, event: &DomainEvent) -> Result<(), AppError> {
        let mut tx = self.db.begin().await?;
        let event_id = self.events.push(&mut tx, event).await?;
        tx.commit().await?;

        // Delivery must never break the business operation that caused it.
        if let Err(e) = self.notifier.handle_event(event, Some(event_id)).await {
            tracing::error!(
                event = event.name(),
                session_id = %event.session_id().map(|s| s.to_string()).unwrap_or_default(),
                error = %e,
                "notification delivery failed"
            );
        }
        Ok(())
    }
}
