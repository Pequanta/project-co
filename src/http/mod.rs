//! Internal (backend-to-bot) HTTP API. The Telegram bot is the user-facing
//! interface; this endpoint lets the backend trigger notification events.
//! It is authenticated and must never be reachable by arbitrary users.

pub mod internal_api;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().nest("/internal", internal_api::router())
}
