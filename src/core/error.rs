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

    #[error("Goal not configured: please edit .orcha/goal.md")]
    GoalNotConfigured,
}
