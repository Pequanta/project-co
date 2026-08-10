//! Session-isolation enforcement.
//!
//! Every session-scoped operation must call these checks *before* touching any
//! session data. This is the security boundary: it lives in the application
//! layer, not in bot logic, so it cannot be bypassed by a different frontend.

use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppError;
use crate::repo::SessionRepo;

/// Fail unless `user_id` is a member of `session_id`.
pub async fn assert_member(
    exec: &mut PgConnection,
    sessions: &dyn SessionRepo,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<(), AppError> {
    if sessions.is_member(exec, session_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Domain(crate::domain::DomainError::NotMember))
    }
}
