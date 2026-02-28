use arc_llm::error::SdkError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] SdkError),

    #[error("Session is closed")]
    SessionClosed,

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Aborted")]
    Aborted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_from_sdk_error() {
        let sdk_err = SdkError::Network {
            message: "connection refused".into(),
        };
        let agent_err = AgentError::from(sdk_err);
        assert!(matches!(agent_err, AgentError::Llm(_)));
        assert!(agent_err.to_string().contains("connection refused"));
    }

    #[test]
    fn session_closed_display() {
        let err = AgentError::SessionClosed;
        assert_eq!(err.to_string(), "Session is closed");
    }

    #[test]
    fn invalid_state_display() {
        let err = AgentError::InvalidState("bad state".into());
        assert_eq!(err.to_string(), "Invalid state: bad state");
    }

    #[test]
    fn tool_execution_display() {
        let err = AgentError::ToolExecution("command failed".into());
        assert_eq!(err.to_string(), "Tool execution error: command failed");
    }

    #[test]
    fn aborted_display() {
        let err = AgentError::Aborted;
        assert_eq!(err.to_string(), "Aborted");
    }
}
