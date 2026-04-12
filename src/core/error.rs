use std::path::PathBuf;

use crate::core::cycle::Phase;

#[derive(Debug, thiserror::Error)]
pub enum OrchaError {
    #[error("Not initialized: .orcha/ directory not found at {path}")]
    NotInitialized { path: PathBuf },

    #[error("Already initialized: .orcha/ directory exists at {path}")]
    AlreadyInitialized { path: PathBuf },

    #[error("Invalid phase transition: cannot move from {from:?} to {to:?}")]
    InvalidPhaseTransition { from: Phase, to: Phase },

    #[error("Status file parse error: {reason}")]
    StatusParseError { reason: String },

    #[error("Task table parse error at line {line}: {reason}")]
    TaskTableParseError { line: usize, reason: String },

    #[error("Unknown profile: {name}")]
    UnknownProfile { name: String },

    #[error("Stop condition reached: {reason}")]
    StopCondition { reason: String },

    #[error("Agent error ({agent}): {message}")]
    AgentError { agent: String, message: String },

    #[error("Agent {agent} not available (no API key configured)")]
    AgentNotAvailable { agent: String },

    #[error("Verification failed: {summary}")]
    VerificationFailed { summary: String },

    #[error("Lock conflict: {holder} currently holds the write lock")]
    LockConflict { holder: String },

    #[error("Machine config error at {path}: {reason}")]
    MachineConfigError { path: PathBuf, reason: String },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::cycle::Phase;

    use super::OrchaError;

    #[test]
    fn not_initialized_error_mentions_path() {
        let err = OrchaError::NotInitialized {
            path: PathBuf::from(".orcha"),
        };
        assert_eq!(err.to_string(), "Not initialized: .orcha/ directory not found at .orcha");
    }

    #[test]
    fn invalid_phase_transition_error_mentions_both_phases() {
        let err = OrchaError::InvalidPhaseTransition {
            from: Phase::Plan,
            to: Phase::Verify,
        };
        let message = err.to_string();
        assert!(message.contains("Plan"));
        assert!(message.contains("Verify"));
    }

    #[test]
    fn task_table_parse_error_mentions_line_and_reason() {
        let err = OrchaError::TaskTableParseError {
            line: 7,
            reason: "Unknown state".into(),
        };
        assert_eq!(err.to_string(), "Task table parse error at line 7: Unknown state");
    }
}
