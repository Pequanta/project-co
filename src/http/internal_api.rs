use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::post;
use axum::Json;
use axum::Router;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::DomainEvent;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    /// One of: `deadline_approaching` | `deadline_reached`
    event: String,
    session_id: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/notifications", post(trigger_notifications))
}

/// `POST /internal/notifications` with
/// `Authorization: Bearer <INTERNAL_API_KEY>` and
/// `{"event": "deadline_approaching", "session_id": "<uuid>"}`.
/// Publishes the event through the outbox; the notification subsystem delivers
/// it to session members. Delivery failures are non-fatal.
async fn trigger_notifications(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TriggerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if auth != Some(&format!("Bearer {}", state.config.internal_api_key)) {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized".to_string()));
    }

    let session_id = Uuid::parse_str(&req.session_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session_id".to_string()))?;

    let event = match req.event.as_str() {
        "deadline_approaching" => DomainEvent::DeadlineApproaching { session_id },
        "deadline_reached" => DomainEvent::DeadlineReached { session_id },
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown event type: {other}"),
            ))
        }
    };

    if let Err(e) = state.publisher.publish(&event).await {
        tracing::error!(error = %e, session_id = %session_id, "internal notification trigger failed");
        return match e {
            AppError::Db(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "db error".to_string())),
            _ => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        };
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}
