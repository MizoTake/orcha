use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::OrchaError;
use crate::machine_config::{LocalLlmCliConfig, LocalLlmMode, MachineConfig};

/// Application configuration resolved from environment variables and .env file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Local LLM (OpenAI-compatible)
    pub local_llm_mode: LocalLlmMode,
    pub local_llm_endpoint: String,
    pub local_llm_model: String,
    pub local_llm_cli: LocalLlmCliConfig,

    // Anthropic (Claude)
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,

    // Google Gemini
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,

    // OpenAI / Codex
    pub openai_api_key: Option<String>,
    pub codex_model: String,
}

impl AppConfig {
    /// Load execution config from `.orcha/orcha.yml` and resolve API keys from env vars.
    pub fn from_orch_dir(orch_dir: &Path) -> anyhow::Result<Self> {
        let cfg_path = MachineConfig::path(orch_dir);
        let machine = MachineConfig::load(orch_dir).map_err(|e| OrchaError::MachineConfigError {
            path: cfg_path,
            reason: e.to_string(),
        })?;
        Ok(Self {
            local_llm_mode: machine.agents.local_llm.mode,
            local_llm_endpoint: machine.agents.local_llm.endpoint,
            local_llm_model: machine.agents.local_llm.model,
            local_llm_cli: machine.agents.local_llm.cli,

            anthropic_api_key: env_api_key(&machine.agents.anthropic.api_key_env),
            anthropic_model: machine.agents.anthropic.model,

            gemini_api_key: env_api_key(&machine.agents.gemini.api_key_env),
            gemini_model: machine.agents.gemini.model,

            openai_api_key: env_api_key(&machine.agents.openai.api_key_env),
            codex_model: machine.agents.openai.model,
        })
    }

    /// Load configuration from environment variables.
    /// Call `dotenvy::dotenv().ok()` before this to load .env file.
    /// Legacy helper; prefers fixed env var names.
    pub fn from_env() -> Self {
        Self {
            local_llm_mode: LocalLlmMode::Http,
            local_llm_endpoint: std::env::var("LOCAL_LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            local_llm_model: std::env::var("LOCAL_LLM_MODEL")
                .unwrap_or_else(|_| "llama3.2".to_string()),
            local_llm_cli: LocalLlmCliConfig::default(),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),

            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_model: std::env::var("GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-2.0-flash".to_string()),

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            codex_model: std::env::var("CODEX_MODEL")
                .unwrap_or_else(|_| "gpt-4.1".to_string()),
        }
    }

    pub fn has_anthropic(&self) -> bool {
        self.anthropic_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_gemini(&self) -> bool {
        self.gemini_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_openai(&self) -> bool {
        self.openai_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }
}

fn env_api_key(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
