use thiserror::Error;

/// Domain-level errors. These encode business rules, not transport problems.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("session not found")]
    SessionNotFound,
    #[error("invalid session key")]
    InvalidSessionKey,
    #[error("session is not open for joining")]
    SessionClosed,
    #[error("you are not a member of this session")]
    NotMember,
    #[error("you are already a member of this session")]
    AlreadyMember,
    #[error("plan not found")]
    PlanNotFound,
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
    #[error("deadline must be in the future")]
    InvalidDeadline,
    #[error("owner cannot leave while other members remain")]
    OwnerCannotLeave,
    #[error("progress message must not be empty")]
    EmptyProgress,
    #[error("session key already exists")]
    SessionKeyTaken,
    #[error("plan title must not be empty")]
    EmptyTitle,
    #[error("project name must not be empty")]
    EmptyProjectName,
}
