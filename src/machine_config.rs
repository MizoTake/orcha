use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::profile::ProfileName;

pub const MACHINE_CONFIG_FILE: &str = "orcha.yml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default)]
    pub local_llm: LocalLlmConfig,
    #[serde(default)]
    pub anthropic: ProviderConfig,
    #[serde(default)]
    pub gemini: ProviderConfig,
    #[serde(default)]
    pub openai: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmConfig {
    #[serde(default)]
    pub mode: LocalLlmMode,
    #[serde(default = "default_local_llm_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_local_llm_model")]
    pub model: String,
    #[serde(default)]
    pub cli: LocalLlmCliConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalLlmMode {
    #[default]
    Http,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmCliConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_prompt_via_stdin")]
    pub prompt_via_stdin: bool,
    #[serde(default)]
    pub model_arg: Option<String>,
    #[serde(default = "default_ensure_no_permission_flags")]
    pub ensure_no_permission_flags: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub profile: Option<ProfileName>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub verification: VerificationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    #[serde(default)]
    pub commands: Vec<String>,
}

impl MachineConfig {
    pub fn path(orch_dir: &Path) -> PathBuf {
        orch_dir.join(MACHINE_CONFIG_FILE)
    }

    pub fn load(orch_dir: &Path) -> anyhow::Result<Self> {
        let path = Self::path(orch_dir);
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            anyhow::anyhow!("Failed to read machine config {}: {}", path.display(), e)
        })?;
        let parsed: MachineConfig = serde_yaml::from_str(&raw).map_err(|e| {
            anyhow::anyhow!("Failed to parse machine config {}: {}", path.display(), e)
        })?;
        Ok(parsed)
    }
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            agents: AgentsConfig::default(),
            execution: ExecutionConfig::default(),
        }
    }
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            local_llm: LocalLlmConfig::default(),
            anthropic: ProviderConfig {
                api_key_env: "ANTHROPIC_API_KEY".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
            },
            gemini: ProviderConfig {
                api_key_env: "GEMINI_API_KEY".to_string(),
                model: "gemini-2.0-flash".to_string(),
            },
            openai: ProviderConfig {
                api_key_env: "OPENAI_API_KEY".to_string(),
                model: "gpt-4.1".to_string(),
            },
        }
    }
}

impl Default for LocalLlmConfig {
    fn default() -> Self {
        Self {
            mode: LocalLlmMode::Http,
            endpoint: default_local_llm_endpoint(),
            model: default_local_llm_model(),
            cli: LocalLlmCliConfig::default(),
        }
    }
}

impl Default for LocalLlmCliConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            prompt_via_stdin: default_prompt_via_stdin(),
            model_arg: None,
            ensure_no_permission_flags: default_ensure_no_permission_flags(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key_env: String::new(),
            model: String::new(),
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            profile: None,
            acceptance_criteria: vec!["Criterion 1".to_string(), "Criterion 2".to_string()],
            verification: VerificationConfig::default(),
        }
    }
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            commands: vec!["echo \"replace with actual verification commands\"".to_string()],
        }
    }
}

fn default_version() -> u32 {
    1
}

fn default_local_llm_endpoint() -> String {
    "http://localhost:11434/v1".to_string()
}

fn default_local_llm_model() -> String {
    "llama3.2".to_string()
}

fn default_prompt_via_stdin() -> bool {
    true
}

fn default_ensure_no_permission_flags() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{LocalLlmMode, MachineConfig};
    use crate::core::profile::ProfileName;

    #[test]
    fn parse_minimal_machine_config() {
        let yml = r#"
version: 1
agents:
  local_llm:
    endpoint: http://localhost:11434/v1
    model: llama3.2
execution:
  acceptance_criteria:
    - first
  verification:
    commands:
      - cargo test
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.execution.acceptance_criteria.len(), 1);
        assert_eq!(cfg.execution.verification.commands, vec!["cargo test"]);
        assert_eq!(cfg.agents.local_llm.model, "llama3.2");
        assert_eq!(cfg.agents.local_llm.mode, LocalLlmMode::Http);
    }

    #[test]
    fn default_contains_verification_commands() {
        let cfg = MachineConfig::default();
        assert!(!cfg.execution.verification.commands.is_empty());
    }

    #[test]
    fn parse_cli_mode_for_local_llm() {
        let yml = r#"
version: 1
agents:
  local_llm:
    mode: cli
    model: llama3.2
    cli:
      command: opencode
      args: ["run"]
      prompt_via_stdin: true
      model_arg: "--model"
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.local_llm.mode, LocalLlmMode::Cli);
        assert_eq!(cfg.agents.local_llm.cli.command, "opencode");
        assert_eq!(cfg.agents.local_llm.cli.model_arg.as_deref(), Some("--model"));
        assert!(cfg.agents.local_llm.cli.ensure_no_permission_flags);
    }

    #[test]
    fn parse_cli_no_permission_flag_override() {
        let yml = r#"
version: 1
agents:
  local_llm:
    mode: cli
    cli:
      command: codex
      ensure_no_permission_flags: false
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert!(!cfg.agents.local_llm.cli.ensure_no_permission_flags);
    }

    #[test]
    fn parse_execution_profile() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: quality_gate
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.execution.profile, Some(ProfileName::QualityGate));
    }
}
