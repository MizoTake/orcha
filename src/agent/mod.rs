pub mod backend;
pub mod router;
pub mod verifier;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Context passed to an agent for processing.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Content of context files (goal.md, status.md, role definition, etc.)
    pub context_files: Vec<ContextFile>,
    /// Logical role for this request (planner, implementer, reviewer, ...)
    pub role: String,
    /// The instruction for this specific phase
    pub instruction: String,
}

#[derive(Debug, Clone)]
pub struct ContextFile {
    pub name: String,
    pub content: String,
}

/// Response from an agent.
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// The markdown response content
    pub content: String,
    /// Model identifier used
    pub model_used: String,
    /// Tokens consumed (if reported by API)
    pub tokens_used: Option<u64>,
    /// Whether this was a paid API call
    pub is_paid: bool,
}

/// Agent kind identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    LocalLlm,
    Claude,
    Gemini,
    Codex,
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentKind::LocalLlm => write!(f, "local_llm"),
            AgentKind::Claude => write!(f, "claude"),
            AgentKind::Gemini => write!(f, "gemini"),
            AgentKind::Codex => write!(f, "codex"),
        }
    }
}

/// The core agent trait matching the spec: Agent.respond(context_files, instruction) -> markdown
#[async_trait]
pub trait Agent: Send + Sync {
    /// Send context and instruction to the agent, receive markdown response.
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse>;

    /// Which kind of agent this is.
    fn kind(&self) -> AgentKind;
}
