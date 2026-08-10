use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::eventing::EventPublisher;
use crate::rate_limit::WebhookRateLimiter;
use crate::repo::DedupeRepo;
use crate::telegram::router::BotRouter;

/// Shared state for all HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub router: Arc<BotRouter>,
    pub publisher: Arc<dyn EventPublisher>,
    pub dedupe: Arc<dyn DedupeRepo>,
    pub webhook_limiter: Arc<WebhookRateLimiter>,
}
