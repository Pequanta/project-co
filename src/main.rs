mod app;
mod config;
mod domain;
mod error;
mod eventing;
mod http;
mod jobs;
mod notify;
mod rate_limit;
mod repo;
mod state;
mod telegram;
mod telemetry;
mod text;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::trace::TraceLayer;

use crate::app::{PlanService, ProgressService, SessionService, UserService};
use crate::config::Config;
use crate::domain::{ProgressCalculator, SimpleProgressCalculator};
use crate::eventing::{EventPublisher, PgEventPublisher};
use crate::notify::{NotificationService, Notifier};
use crate::rate_limit::WebhookRateLimiter;
use crate::repo::{
    pg::{
        PgConversationRepo, PgDedupeRepo, PgNotificationRepo, PgPlanCompletionRepo, PgPlanRepo,
        PgProgressRepo, PgSessionRepo, PgUserRepo,
    },
    ConversationRepo, DedupeRepo, NotificationRepo, PlanCompletionRepo, PlanRepo, ProgressRepo,
    SessionRepo, UserRepo,
};
use crate::state::AppState;
use crate::telegram::gateway::{ReqwestGateway, TelegramGateway};
use crate::telegram::router::BotRouter;
use crate::telegram::webhook::webhook_handler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let config = Arc::new(Config::from_env()?);
    tracing::info!(addr = %config.http_addr, "starting project-co");

    // Persistence: PostgreSQL is the source of truth.
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database ready, migrations applied");

    // Outbound Telegram adapter.
    let gateway: Arc<dyn TelegramGateway> = Arc::new(ReqwestGateway::new(config.bot_token.clone()));
    if let Some(url) = &config.webhook_url {
        gateway.set_webhook(url, &config.webhook_secret).await?;
        tracing::info!(url = %url, "webhook registered with Telegram");
    }

    // Repositories.
    let users_repo: Arc<dyn UserRepo> = Arc::new(PgUserRepo);
    let sessions_repo: Arc<dyn SessionRepo> = Arc::new(PgSessionRepo);
    let plans_repo: Arc<dyn PlanRepo> = Arc::new(PgPlanRepo);
    let completions_repo: Arc<dyn PlanCompletionRepo> = Arc::new(PgPlanCompletionRepo);
    let progress_repo: Arc<dyn ProgressRepo> = Arc::new(PgProgressRepo);
    let notifications_repo: Arc<dyn NotificationRepo> = Arc::new(PgNotificationRepo);
    let conversations_repo: Arc<dyn ConversationRepo> = Arc::new(PgConversationRepo);
    let dedupe_repo: Arc<dyn DedupeRepo> = Arc::new(PgDedupeRepo);

    let calculator: Arc<dyn ProgressCalculator> = Arc::new(SimpleProgressCalculator);

    // Notification subsystem + event system.
    let notification_service = Arc::new(NotificationService::new(
        pool.clone(),
        users_repo.clone(),
        sessions_repo.clone(),
        plans_repo.clone(),
        completions_repo.clone(),
        progress_repo.clone(),
        notifications_repo.clone(),
        gateway.clone(),
        calculator.clone(),
    ));
    let notifier: Arc<dyn Notifier> = notification_service.clone();
    let publisher: Arc<dyn EventPublisher> =
        Arc::new(PgEventPublisher::new(pool.clone(), notifier));

    // Application services.
    let user_service = Arc::new(UserService::with_repo(pool.clone(), users_repo.clone()));
    let session_service = Arc::new(SessionService::new(
        pool.clone(),
        users_repo.clone(),
        sessions_repo.clone(),
        plans_repo.clone(),
        completions_repo.clone(),
        progress_repo.clone(),
        publisher.clone(),
        calculator.clone(),
    ));
    let plan_service = Arc::new(PlanService::new(
        pool.clone(),
        sessions_repo.clone(),
        plans_repo.clone(),
        completions_repo.clone(),
        publisher.clone(),
    ));
    let progress_service = Arc::new(ProgressService::new(
        pool.clone(),
        sessions_repo.clone(),
        progress_repo.clone(),
        publisher.clone(),
    ));

    // Telegram router (inbound updates).
    let router = Arc::new(BotRouter::new(
        pool.clone(),
        user_service.clone(),
        session_service.clone(),
        plan_service.clone(),
        progress_service.clone(),
        conversations_repo.clone(),
        gateway.clone(),
    ));

    // Deadline reminders background job.
    if config.reminder_window_hours > 0 && config.reminder_interval_hours > 0 {
        let db = pool.clone();
        let sessions = sessions_repo.clone();
        let publisher = publisher.clone();
        let window = config.reminder_window_hours;
        let interval = config.reminder_interval_hours;
        tokio::spawn(async move {
            jobs::deadline_reminder_loop(db, sessions, publisher, window, interval).await;
        });
        tracing::info!(
            window_hours = window,
            interval_hours = interval,
            "deadline reminder job started"
        );
    }

    let state = AppState {
        config: config.clone(),
        db: pool,
        router,
        publisher,
        dedupe: dedupe_repo,
        webhook_limiter: Arc::new(WebhookRateLimiter::new(
            config.webhook_rate_limit_per_minute,
        )),
    };

    let app = Router::new()
        .route("/webhook", post(webhook_handler))
        .route("/healthz", get(healthz))
        .merge(http::router())
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.http_addr).await?;
    tracing::info!(addr = %config.http_addr, "listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn healthz(State(_state): State<AppState>) -> impl IntoResponse {
    StatusCode::OK
}
