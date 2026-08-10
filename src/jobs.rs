//! Background jobs. The only scheduler is the deadline-reminder sweep; it runs
//! in-process. It publishes `DeadlineApproaching`/`DeadlineReached` events
//! through the outbox so the notification subsystem handles delivery.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;

use crate::domain::DomainEvent;
use crate::error::AppError;
use crate::eventing::EventPublisher;
use crate::repo::SessionRepo;

/// Loop forever, sweeping sessions for due reminders.
pub async fn deadline_reminder_loop(
    db: PgPool,
    sessions: Arc<dyn SessionRepo>,
    publisher: Arc<dyn EventPublisher>,
    window_hours: i64,
    interval_hours: u64,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_hours * 3600));
    ticker.tick().await; // consume the immediate first tick
    loop {
        ticker.tick().await;
        if let Err(e) = run_once(&db, sessions.as_ref(), publisher.as_ref(), window_hours).await {
            tracing::error!(error = %e, "deadline reminder sweep failed");
        }
    }
}

pub async fn run_once(
    db: &PgPool,
    sessions: &dyn SessionRepo,
    publisher: &dyn EventPublisher,
    window_hours: i64,
) -> Result<(), AppError> {
    let mut conn = db.acquire().await?;
    let due = sessions.due_for_reminder(&mut conn, window_hours).await?;
    for session in due {
        let reached = session.deadline <= Utc::now();
        let event = if reached {
            DomainEvent::DeadlineReached {
                session_id: session.id,
            }
        } else {
            DomainEvent::DeadlineApproaching {
                session_id: session.id,
            }
        };
        let kind = event.name();
        match publisher.publish(&event).await {
            Ok(()) => {
                sessions.mark_reminded(&mut conn, session.id).await?;
                tracing::info!(session_id = %session.id, event = kind, "deadline reminder sent");
            }
            Err(e) => {
                tracing::warn!(session_id = %session.id, event = kind, error = %e, "deadline reminder failed");
            }
        }
    }
    Ok(())
}
