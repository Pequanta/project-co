//! The Telegram webhook entrypoint (serverless/webhook bot, no polling).
//!
//! Every request is: validated against the secret token header → deduplicated
//! by `update_id` → routed to the application via `BotRouter`.

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;
use crate::telegram::types::Update;

pub async fn webhook_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !state.webhook_limiter.check(peer.ip()) {
        tracing::warn!(ip = %peer.ip(), "webhook rate limit exceeded");
        return (StatusCode::TOO_MANY_REQUESTS, "too many requests").into_response();
    }

    // 1. Validate the request per Telegram's recommended mechanism.
    let secret = headers
        .get("x-telegram-bot-api-secret-token")
        .and_then(|v| v.to_str().ok());
    if secret != Some(state.config.webhook_secret.as_str()) {
        tracing::warn!("rejected webhook request: bad secret token");
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    // 2. Parse the update. On malformed payloads, acknowledge anyway so
    //    Telegram stops retrying; log for diagnosis.
    let update: Update = match serde_json::from_slice(&body) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "could not parse Telegram update");
            return StatusCode::OK.into_response();
        }
    };

    // 3. Deduplicate duplicate deliveries (Telegram retries on failure).
    let mut conn = match state.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "could not acquire db connection");
            return StatusCode::OK.into_response();
        }
    };
    match state.dedupe.mark_seen(&mut conn, update.update_id).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::debug!(update_id = update.update_id, "duplicate update ignored");
            return StatusCode::OK.into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "dedupe check failed");
            return StatusCode::OK.into_response();
        }
    }

    // 4. Route to the application. Reply 200 either way: the update has been
    //    persisted, and notification retry is the outbox's job, not Telegram's.
    if let Err(e) = state.router.handle_update(&update).await {
        tracing::error!(update_id = update.update_id, error = %e, "update handling failed");
    }
    StatusCode::OK.into_response()
}
