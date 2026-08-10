//! Serverless conversation state machine. Because the bot is stateless per
//! invocation, flow state lives in the `bot_conversations` table, keyed by
//! app user. Payload holds per-flow data (draft fields, chosen session, ...).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvState {
    Idle,
    AwaitingProjectName,
    AwaitingProjectDescription,
    AwaitingProjectDeadline,
    AwaitingInitialPlans,
    AwaitingCreateSessionKey,
    AwaitingSessionKey,
    AwaitingProgressMessage,
    AwaitingPlanTitle,
}

impl ConvState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConvState::Idle => "idle",
            ConvState::AwaitingProjectName => "awaiting_project_name",
            ConvState::AwaitingProjectDescription => "awaiting_project_description",
            ConvState::AwaitingProjectDeadline => "awaiting_project_deadline",
            ConvState::AwaitingInitialPlans => "awaiting_initial_plans",
            ConvState::AwaitingCreateSessionKey => "awaiting_create_session_key",
            ConvState::AwaitingSessionKey => "awaiting_session_key",
            ConvState::AwaitingProgressMessage => "awaiting_progress_message",
            ConvState::AwaitingPlanTitle => "awaiting_plan_title",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "awaiting_project_name" => ConvState::AwaitingProjectName,
            "awaiting_project_description" => ConvState::AwaitingProjectDescription,
            "awaiting_project_deadline" => ConvState::AwaitingProjectDeadline,
            "awaiting_initial_plans" => ConvState::AwaitingInitialPlans,
            "awaiting_create_session_key" => ConvState::AwaitingCreateSessionKey,
            "awaiting_session_key" => ConvState::AwaitingSessionKey,
            "awaiting_progress_message" => ConvState::AwaitingProgressMessage,
            "awaiting_plan_title" => ConvState::AwaitingPlanTitle,
            _ => ConvState::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrips() {
        for s in [
            ConvState::Idle,
            ConvState::AwaitingProjectName,
            ConvState::AwaitingInitialPlans,
            ConvState::AwaitingCreateSessionKey,
            ConvState::AwaitingProgressMessage,
        ] {
            assert_eq!(ConvState::from_str(s.as_str()), s);
        }
        assert_eq!(ConvState::from_str("bogus"), ConvState::Idle);
    }
}
