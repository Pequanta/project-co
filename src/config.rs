use std::env;

use crate::error::AppError;

/// Everything configurable at runtime comes from the environment. No defaults
/// for secrets; the process refuses to start without them.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bot_token: String,
    /// Secret token Telegram sends as `X-Telegram-Bot-Api-Secret-Token` on
    /// every webhook request. Requests without it are rejected.
    pub webhook_secret: String,
    /// If set, the bot registers its webhook on startup. Render deployments
    /// fall back to the platform-provided external URL plus `/webhook`.
    pub webhook_url: Option<String>,
    pub http_addr: std::net::SocketAddr,
    /// Bearer token protecting the internal API.
    pub internal_api_key: String,
    /// Hours before the deadline at which reminders fire (0 disables).
    pub reminder_window_hours: i64,
    /// How often the background reminder job runs.
    pub reminder_interval_hours: u64,
    /// Maximum webhook requests permitted from one source IP each minute.
    pub webhook_rate_limit_per_minute: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let require = |k: &str| -> Result<String, AppError> {
            env::var(k).map_err(|_| AppError::Config(format!("missing required env var {k}")))
        };

        let http_addr = env::var("HTTP_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("invalid HTTP_ADDR: {e}")))?;

        let reminder_window_hours = env::var("DEADLINE_REMINDER_WINDOW_HOURS")
            .ok()
            .map(|v| v.parse())
            .transpose()
            .map_err(|e| AppError::Config(format!("invalid DEADLINE_REMINDER_WINDOW_HOURS: {e}")))?
            .unwrap_or(72);

        let reminder_interval_hours = env::var("DEADLINE_REMINDER_INTERVAL_HOURS")
            .ok()
            .map(|v| v.parse())
            .transpose()
            .map_err(|e| {
                AppError::Config(format!("invalid DEADLINE_REMINDER_INTERVAL_HOURS: {e}"))
            })?
            .unwrap_or(6);
        let webhook_rate_limit_per_minute = env::var("WEBHOOK_RATE_LIMIT_PER_MINUTE")
            .ok()
            .map(|v| v.parse())
            .transpose()
            .map_err(|e| AppError::Config(format!("invalid WEBHOOK_RATE_LIMIT_PER_MINUTE: {e}")))?
            .unwrap_or(120);
        if webhook_rate_limit_per_minute == 0 {
            return Err(AppError::Config(
                "WEBHOOK_RATE_LIMIT_PER_MINUTE must be greater than zero".to_string(),
            ));
        }

        Ok(Config {
            database_url: require("DATABASE_URL")?,
            bot_token: require("BOT_TOKEN")?,
            webhook_secret: require("BOT_WEBHOOK_SECRET")?,
            webhook_url: env::var("BOT_WEBHOOK_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    env::var("RENDER_EXTERNAL_URL")
                        .ok()
                        .filter(|s| !s.is_empty())
                        .map(|url| format!("{}/webhook", url.trim_end_matches('/')))
                }),
            http_addr,
            internal_api_key: require("INTERNAL_API_KEY")?,
            reminder_window_hours,
            reminder_interval_hours,
            webhook_rate_limit_per_minute,
        })
    }
}
