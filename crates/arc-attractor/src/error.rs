use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AttractorError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Handler error: {0}")]
    Handler(String),

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),

    #[error("Stylesheet error: {0}")]
    Stylesheet(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Pipeline cancelled")]
    Cancelled,
}

impl AttractorError {
    /// Whether this error category is retryable (transient) or terminal.
    ///
    /// Retryable: Handler (transient handler failures), Engine (could be transient),
    ///            Io (network/disk issues are often transient).
    /// Terminal:  Parse, Validation, Stylesheet (configuration errors),
    ///            Checkpoint (storage integrity), Cancelled (explicit cancellation).
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::Handler(_) | Self::Engine(_) | Self::Io(_) => true,
            Self::Parse(_)
            | Self::Validation(_)
            | Self::Stylesheet(_)
            | Self::Checkpoint(_)
            | Self::Cancelled => false,
        }
    }
}

impl From<std::io::Error> for AttractorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AttractorError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_display() {
        let err = AttractorError::Parse("unexpected token".to_string());
        assert_eq!(err.to_string(), "Parse error: unexpected token");
    }

    #[test]
    fn validation_error_display() {
        let err = AttractorError::Validation("missing start node".to_string());
        assert_eq!(err.to_string(), "Validation error: missing start node");
    }

    #[test]
    fn engine_error_display() {
        let err = AttractorError::Engine("no outgoing edge".to_string());
        assert_eq!(err.to_string(), "Engine error: no outgoing edge");
    }

    #[test]
    fn handler_error_display() {
        let err = AttractorError::Handler("LLM call failed".to_string());
        assert_eq!(err.to_string(), "Handler error: LLM call failed");
    }

    #[test]
    fn checkpoint_error_display() {
        let err = AttractorError::Checkpoint("file not found".to_string());
        assert_eq!(err.to_string(), "Checkpoint error: file not found");
    }

    #[test]
    fn io_error_display() {
        let err = AttractorError::Io("permission denied".to_string());
        assert_eq!(err.to_string(), "I/O error: permission denied");
    }

    #[test]
    fn io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = AttractorError::from(io_err);
        assert!(matches!(err, AttractorError::Io(_)));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn result_type_alias_works() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: Result<i32> = Err(AttractorError::Parse("bad".to_string()));
        assert!(err.is_err());
    }

    #[test]
    fn cancelled_error_display() {
        let err = AttractorError::Cancelled;
        assert_eq!(err.to_string(), "Pipeline cancelled");
    }

    #[test]
    fn cancelled_is_not_retryable() {
        assert!(!AttractorError::Cancelled.is_retryable());
    }

    #[test]
    fn is_retryable_terminal_errors() {
        assert!(!AttractorError::Parse("bad".to_string()).is_retryable());
        assert!(!AttractorError::Validation("bad".to_string()).is_retryable());
        assert!(!AttractorError::Stylesheet("bad".to_string()).is_retryable());
        assert!(!AttractorError::Checkpoint("bad".to_string()).is_retryable());
    }

    #[test]
    fn is_retryable_transient_errors() {
        assert!(AttractorError::Handler("timeout".to_string()).is_retryable());
        assert!(AttractorError::Engine("transient".to_string()).is_retryable());
        assert!(AttractorError::Io("connection reset".to_string()).is_retryable());
    }
}
