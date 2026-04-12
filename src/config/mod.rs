use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::OrchaError;
use crate::machine_config::{LocalLlmCliConfig, MachineConfig, ProviderMode};

/// Application configuration resolved from environment variables and .env file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Local LLM (OpenAI-compatible)
    pub local_llm_mode: ProviderMode,
    pub local_llm_endpoint: String,
    pub local_llm_model: String,
    pub local_llm_cli: LocalLlmCliConfig,

    // Anthropic (Claude)
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub anthropic_mode: ProviderMode,
    pub anthropic_cli: LocalLlmCliConfig,

    // Google Gemini
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub gemini_mode: ProviderMode,
    pub gemini_cli: LocalLlmCliConfig,

    // OpenAI / Codex
    pub openai_api_key: Option<String>,
    pub codex_model: String,
    pub openai_mode: ProviderMode,
    pub openai_cli: LocalLlmCliConfig,
}

impl AppConfig {
    /// Load execution config from `.orcha/orcha.yml` and resolve API keys from env vars.
    pub fn from_orch_dir(orch_dir: &Path) -> anyhow::Result<Self> {
        let cfg_path = MachineConfig::path(orch_dir);
        let machine =
            MachineConfig::load(orch_dir).map_err(|e| OrchaError::MachineConfigError {
                path: cfg_path,
                reason: e.to_string(),
            })?;
        let local_llm_model = resolve_local_llm_model(&machine);
        let local_llm_mode = machine.agents.local_llm.mode.clone();
        let local_llm_endpoint = machine.agents.local_llm.endpoint.clone();
        let local_llm_cli = machine.agents.local_llm.cli.clone();

        Ok(Self {
            local_llm_mode,
            local_llm_endpoint,
            local_llm_model,
            local_llm_cli,

            anthropic_api_key: env_api_key(&machine.agents.claude.api_key_env),
            anthropic_model: machine.agents.claude.model.clone(),
            anthropic_mode: machine.agents.claude.mode.clone(),
            anthropic_cli: machine.agents.claude.cli.clone(),

            gemini_api_key: env_api_key(&machine.agents.gemini.api_key_env),
            gemini_model: machine.agents.gemini.model.clone(),
            gemini_mode: machine.agents.gemini.mode.clone(),
            gemini_cli: machine.agents.gemini.cli.clone(),

            openai_api_key: env_api_key(&machine.agents.codex.api_key_env),
            codex_model: machine.agents.codex.model.clone(),
            openai_mode: machine.agents.codex.mode.clone(),
            openai_cli: machine.agents.codex.cli.clone(),
        })
    }

    /// Load configuration from environment variables.
    /// Call `dotenvy::dotenv().ok()` before this to load .env file.
    /// Legacy helper; prefers fixed env var names.
    pub fn from_env() -> Self {
        Self {
            local_llm_mode: ProviderMode::Http,
            local_llm_endpoint: std::env::var("LOCAL_LLM_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            local_llm_model: std::env::var("LOCAL_LLM_MODEL")
                .unwrap_or_else(|_| "llama3.2".to_string()),
            local_llm_cli: LocalLlmCliConfig::default(),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),
            anthropic_mode: ProviderMode::Http,
            anthropic_cli: LocalLlmCliConfig::default(),

            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_model: std::env::var("GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-2.0-flash".to_string()),
            gemini_mode: ProviderMode::Http,
            gemini_cli: LocalLlmCliConfig::default(),

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            codex_model: std::env::var("CODEX_MODEL").unwrap_or_else(|_| "gpt-4.1".to_string()),
            openai_mode: ProviderMode::Http,
            openai_cli: LocalLlmCliConfig::default(),
        }
    }

    pub fn has_anthropic(&self) -> bool {
        matches!(self.anthropic_mode, ProviderMode::Cli)
            || self
                .anthropic_api_key
                .as_ref()
                .is_some_and(|k| !k.is_empty())
    }

    pub fn has_gemini(&self) -> bool {
        matches!(self.gemini_mode, ProviderMode::Cli)
            || self.gemini_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_openai(&self) -> bool {
        matches!(self.openai_mode, ProviderMode::Cli)
            || self.openai_api_key.as_ref().is_some_and(|k| !k.trim().is_empty())
    }
}

fn env_api_key(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn resolve_local_llm_model(machine: &MachineConfig) -> String {
    if let Some(model) = machine
        .agents
        .local_llm
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        return model.to_string();
    }

    match machine.agents.local_llm.mode {
        ProviderMode::Http => "llama3.2".to_string(),
        ProviderMode::Cli => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::AppConfig;

    #[test]
    fn cli_without_model_keeps_model_empty() {
        let dir = TempDir::new().unwrap();
        let yml = r#"
version: 1
agents:
  local_llm:
    mode: cli
    cli:
      command: codex
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;
        std::fs::write(dir.path().join("orcha.yml"), yml).unwrap();

        let cfg = AppConfig::from_orch_dir(Path::new(dir.path())).unwrap();
        assert!(cfg.local_llm_model.is_empty());
    }

    fn base_config() -> AppConfig {
        AppConfig {
            local_llm_mode: crate::machine_config::ProviderMode::Http,
            local_llm_endpoint: String::new(),
            local_llm_model: String::new(),
            local_llm_cli: crate::machine_config::LocalLlmCliConfig::default(),
            anthropic_api_key: None,
            anthropic_model: String::new(),
            anthropic_mode: crate::machine_config::ProviderMode::Http,
            anthropic_cli: crate::machine_config::LocalLlmCliConfig::default(),
            gemini_api_key: None,
            gemini_model: String::new(),
            gemini_mode: crate::machine_config::ProviderMode::Http,
            gemini_cli: crate::machine_config::LocalLlmCliConfig::default(),
            openai_api_key: None,
            codex_model: String::new(),
            openai_mode: crate::machine_config::ProviderMode::Http,
            openai_cli: crate::machine_config::LocalLlmCliConfig::default(),
        }
    }

    #[test]
    fn has_anthropic_true_when_api_key_set() {
        let mut cfg = base_config();
        cfg.anthropic_api_key = Some("sk-test".to_string());
        assert!(cfg.has_anthropic());
    }

    #[test]
    fn has_anthropic_false_when_api_key_empty_string() {
        let mut cfg = base_config();
        cfg.anthropic_api_key = Some(String::new());
        assert!(!cfg.has_anthropic());
    }

    #[test]
    fn has_anthropic_false_when_api_key_none() {
        let cfg = base_config();
        assert!(!cfg.has_anthropic());
    }

    #[test]
    fn has_anthropic_true_when_mode_is_cli_regardless_of_key() {
        let mut cfg = base_config();
        cfg.anthropic_mode = crate::machine_config::ProviderMode::Cli;
        cfg.anthropic_api_key = None;
        assert!(cfg.has_anthropic());
    }

    #[test]
    fn has_gemini_true_when_api_key_set() {
        let mut cfg = base_config();
        cfg.gemini_api_key = Some("key".to_string());
        assert!(cfg.has_gemini());
    }

    #[test]
    fn has_gemini_false_when_api_key_none() {
        let cfg = base_config();
        assert!(!cfg.has_gemini());
    }

    #[test]
    fn has_gemini_true_when_mode_is_cli() {
        let mut cfg = base_config();
        cfg.gemini_mode = crate::machine_config::ProviderMode::Cli;
        assert!(cfg.has_gemini());
    }

    #[test]
    fn has_openai_true_when_api_key_set() {
        let mut cfg = base_config();
        cfg.openai_api_key = Some("key".to_string());
        assert!(cfg.has_openai());
    }

    #[test]
    fn has_openai_false_when_api_key_empty() {
        let mut cfg = base_config();
        cfg.openai_api_key = Some("   ".to_string());
        assert!(!cfg.has_openai());
    }

    #[test]
    fn has_openai_true_when_mode_is_cli() {
        let mut cfg = base_config();
        cfg.openai_mode = crate::machine_config::ProviderMode::Cli;
        assert!(cfg.has_openai());
    }

    #[test]
    fn http_without_model_uses_local_default_model() {
        let dir = TempDir::new().unwrap();
        let yml = r#"
version: 1
agents:
  local_llm:
    mode: http
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;
        std::fs::write(dir.path().join("orcha.yml"), yml).unwrap();

        let cfg = AppConfig::from_orch_dir(Path::new(dir.path())).unwrap();
        assert_eq!(cfg.local_llm_model, "llama3.2");
    }
}
