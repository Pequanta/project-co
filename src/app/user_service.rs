use std::sync::Arc;

use sqlx::PgPool;

use crate::domain::User;
use crate::error::AppError;
use crate::repo::UserRepo;

/// Maps Telegram users to application users. `/start` (and every update) calls
/// `ensure_registered` so unregistered users are always guided correctly.
pub struct UserService {
    db: PgPool,
    users: Arc<dyn UserRepo>,
}

impl UserService {
    pub fn with_repo(db: PgPool, users: Arc<dyn UserRepo>) -> Self {
        Self { db, users }
    }

    /// Upsert by telegram id. `display_name` falls back to username.
    pub async fn ensure_registered(
        &self,
        telegram_user_id: i64,
        telegram_username: Option<String>,
        first_name: Option<&str>,
        last_name: Option<&str>,
    ) -> Result<User, AppError> {
        let mut conn = self.db.acquire().await?;
        let display_name = build_display_name(first_name, last_name, telegram_username.as_deref());
        let user = self
            .users
            .upsert(
                &mut conn,
                telegram_user_id,
                telegram_username.as_deref(),
                &display_name,
            )
            .await?;
        Ok(user)
    }
}

pub fn build_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    username: Option<&str>,
) -> String {
    match (first_name, last_name) {
        (Some(f), Some(l)) => format!("{f} {l}").trim().to_string(),
        (Some(f), None) => f.trim().to_string(),
        (None, Some(l)) => l.trim().to_string(),
        (None, None) => username
            .map(|u| u.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "User".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_prioritizes_real_name() {
        assert_eq!(
            build_display_name(Some("Alice"), Some("Smith"), Some("alice123")),
            "Alice Smith"
        );
        assert_eq!(build_display_name(Some("Alice"), None, None), "Alice");
        assert_eq!(build_display_name(None, None, Some("bob")), "bob");
        assert_eq!(build_display_name(None, None, None), "User");
    }
}
