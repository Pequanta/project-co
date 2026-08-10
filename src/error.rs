//! Top-level application error. Wraps transport and domain failures.
use thiserror::Error;

use crate::domain::DomainError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("telegram gateway error: {0}")]
    Gateway(#[from] crate::telegram::gateway::GatewayError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("internal error: {0}")]
    Internal(String),
}
