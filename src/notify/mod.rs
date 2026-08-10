//! Notification subsystem: consumes domain events and delivers messages to
//! session members through the Telegram gateway. Delivery failures are
//! recorded in `notifications` and never propagate as fatal errors.

use async_trait::async_trait;

use crate::domain::DomainEvent;
use crate::error::AppError;

pub mod messages;
pub mod notification_service;

pub use notification_service::NotificationService;

/// Consumed by the event system to trigger notification delivery.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// `event_id` is the outbox row id (if any) for audit/retry linkage.
    async fn handle_event(
        &self,
        event: &DomainEvent,
        event_id: Option<i64>,
    ) -> Result<(), AppError>;
}
