use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::profile::{ProfileName, ProfileRules};

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
    #[serde(default, alias = "anthropic")]
    pub claude: ProviderConfig,
    #[serde(default)]
    pub gemini: ProviderConfig,
    #[serde(default, alias = "openai")]
    pub codex: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmConfig {
    #[serde(default)]
    pub mode: LocalLlmMode,
    #[serde(default = "default_local_llm_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cli: LocalLlmCliConfig,
}

/// Backwards-compatible alias; `LocalLlmConfig.mode` now uses the shared `ProviderMode`.
pub type LocalLlmMode = ProviderMode;

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
    #[serde(default)]
    pub mode: ProviderMode,
    #[serde(default)]
    pub cli: LocalLlmCliConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    #[default]
    Http,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub profile: Option<ProfileName>,
    #[serde(default)]
    pub profile_strategy: ProfileStrategyConfig,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub verification: VerificationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileStrategyConfig {
    #[serde(default)]
    pub alternating: Vec<ProfileName>,
    #[serde(default)]
    pub every_n_cycles: Vec<EveryNCycleProfileSwitch>,
    #[serde(default)]
    pub mixins: Vec<ProfileMixinConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EveryNCycleProfileSwitch {
    pub interval: u32,
    pub profile: ProfileName,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMixinConfig {
    pub from: ProfileName,
    #[serde(default)]
    pub fields: Vec<ProfileRuleField>,
    #[serde(default)]
    pub every_n_cycles: Option<u32>,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRuleField {
    DefaultAgent,
    ReviewAgent,
    Escalation,
    SecurityGate,
    SizeGate,
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

impl ExecutionConfig {
    pub fn resolve_profile_name(&self, cycle: u32, fallback: ProfileName) -> ProfileName {
        let mut current = self.profile.unwrap_or(fallback);

        if !self.profile_strategy.alternating.is_empty() {
            let idx = (cycle as usize) % self.profile_strategy.alternating.len();
            current = self.profile_strategy.alternating[idx];
        }

        for switch in &self.profile_strategy.every_n_cycles {
            if switch.interval == 0 {
                continue;
            }
            if cycle >= switch.offset && (cycle - switch.offset) % switch.interval == 0 {
                current = switch.profile;
            }
        }

        current
    }

    pub fn resolve_profile_rules(&self, cycle: u32, fallback: ProfileName) -> ProfileRules {
        let base = self.resolve_profile_name(cycle, fallback);
        let mut resolved = ProfileRules::from_name(base);

        for mixin in &self.profile_strategy.mixins {
            if !mixin_applies(cycle, mixin) {
                continue;
            }
            let source = ProfileRules::from_name(mixin.from);
            apply_mixin(&mut resolved, &source, &mixin.fields);
        }

        resolved
    }

    pub fn has_profile_strategy(&self) -> bool {
        !self.profile_strategy.alternating.is_empty()
            || !self.profile_strategy.every_n_cycles.is_empty()
            || !self.profile_strategy.mixins.is_empty()
    }
}

fn mixin_applies(cycle: u32, mixin: &ProfileMixinConfig) -> bool {
    match mixin.every_n_cycles {
        Some(interval) if interval > 0 => {
            cycle >= mixin.offset && (cycle - mixin.offset) % interval == 0
        }
        Some(_) => false,
        None => true,
    }
}

fn apply_mixin(target: &mut ProfileRules, source: &ProfileRules, fields: &[ProfileRuleField]) {
    let apply_all = fields.is_empty();

    if apply_all || fields.contains(&ProfileRuleField::DefaultAgent) {
        target.default_agent = source.default_agent;
    }
    if apply_all || fields.contains(&ProfileRuleField::ReviewAgent) {
        target.review_agent = source.review_agent;
    }
    if apply_all || fields.contains(&ProfileRuleField::Escalation) {
        target.escalation = source.escalation.clone();
    }
    if apply_all || fields.contains(&ProfileRuleField::SecurityGate) {
        target.security_gate_enabled = source.security_gate_enabled;
    }
    if apply_all || fields.contains(&ProfileRuleField::SizeGate) {
        target.size_gate_enabled = source.size_gate_enabled;
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
            claude: ProviderConfig {
                api_key_env: "ANTHROPIC_API_KEY".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                mode: ProviderMode::Http,
                cli: LocalLlmCliConfig::default(),
            },
            gemini: ProviderConfig {
                api_key_env: "GEMINI_API_KEY".to_string(),
                model: "gemini-2.0-flash".to_string(),
                mode: ProviderMode::Http,
                cli: LocalLlmCliConfig::default(),
            },
            codex: ProviderConfig {
                api_key_env: "OPENAI_API_KEY".to_string(),
                model: "gpt-4.1".to_string(),
                mode: ProviderMode::Http,
                cli: LocalLlmCliConfig::default(),
            },
        }
    }
}

impl Default for LocalLlmConfig {
    fn default() -> Self {
        Self {
            mode: ProviderMode::Http,
            endpoint: default_local_llm_endpoint(),
            model: None,
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
            mode: ProviderMode::Http,
            cli: LocalLlmCliConfig::default(),
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            profile: None,
            profile_strategy: ProfileStrategyConfig::default(),
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

fn default_prompt_via_stdin() -> bool {
    true
}

fn default_ensure_no_permission_flags() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{ExecutionConfig, MachineConfig, ProfileRuleField, ProviderMode};
    use crate::core::profile::{AgentPreference, ProfileName};

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
        assert_eq!(cfg.agents.local_llm.model.as_deref(), Some("llama3.2"));
        assert_eq!(cfg.agents.local_llm.mode, ProviderMode::Http);
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
        assert_eq!(cfg.agents.local_llm.mode, ProviderMode::Cli);
        assert_eq!(cfg.agents.local_llm.cli.command, "opencode");
        assert_eq!(
            cfg.agents.local_llm.cli.model_arg.as_deref(),
            Some("--model")
        );
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
    fn parse_cli_mode_for_claude_provider() {
        let yml = r#"
version: 1
agents:
  claude:
    mode: cli
    model: claude-sonnet-4-20250514
    cli:
      command: claude
      args: ["--dangerously-skip-permissions"]
      prompt_via_stdin: false
      model_arg: "--model"
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.claude.mode, ProviderMode::Cli);
        assert_eq!(cfg.agents.claude.cli.command, "claude");
        assert!(!cfg.agents.claude.cli.prompt_via_stdin);
        assert_eq!(
            cfg.agents.claude.cli.model_arg.as_deref(),
            Some("--model")
        );
        assert_eq!(cfg.agents.claude.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn parse_cli_mode_for_codex_provider() {
        let yml = r#"
version: 1
agents:
  codex:
    mode: cli
    model: codex
    cli:
      command: codex
      args: ["exec", "--sandbox", "danger-full-access"]
      prompt_via_stdin: false
      ensure_no_permission_flags: false
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.codex.mode, ProviderMode::Cli);
        assert_eq!(cfg.agents.codex.cli.command, "codex");
        assert!(!cfg.agents.codex.cli.ensure_no_permission_flags);
        assert_eq!(cfg.agents.codex.model, "codex");
    }

    #[test]
    fn provider_defaults_to_http_mode() {
        let yml = r#"
version: 1
agents:
  claude:
    api_key_env: ANTHROPIC_API_KEY
    model: claude-sonnet-4-20250514
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.claude.mode, ProviderMode::Http);
        assert!(cfg.agents.claude.cli.command.is_empty());
    }

    #[test]
    fn legacy_anthropic_key_maps_to_claude() {
        let yml = r#"
version: 1
agents:
  anthropic:
    mode: cli
    model: claude-sonnet-4-20250514
    cli:
      command: claude
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.claude.mode, ProviderMode::Cli);
        assert_eq!(cfg.agents.claude.cli.command, "claude");
    }

    #[test]
    fn legacy_openai_key_maps_to_codex() {
        let yml = r#"
version: 1
agents:
  openai:
    mode: cli
    model: codex
    cli:
      command: codex
execution:
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(cfg.agents.codex.mode, ProviderMode::Cli);
        assert_eq!(cfg.agents.codex.cli.command, "codex");
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

    #[test]
    fn parse_execution_profile_with_impl_review_name() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: claude_impl_opencode_review
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution.profile,
            Some(ProfileName::ClaudeImplOpencodeReview)
        );
    }

    #[test]
    fn parse_execution_profile_with_legacy_swapped_alias() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: opencode_claude_swapped
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution.profile,
            Some(ProfileName::ClaudeImplOpencodeReview)
        );
    }

    #[test]
    fn parse_execution_profile_with_opencode_impl_name() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: opencode_impl_claude_review
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution.profile,
            Some(ProfileName::OpencodeImplClaudeReview)
        );
    }

    #[test]
    fn parse_execution_profile_with_legacy_opencode_alias() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: opencode_claude
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution.profile,
            Some(ProfileName::OpencodeImplClaudeReview)
        );
    }

    #[test]
    fn resolve_profile_name_with_alternating() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: cheap_checkpoints
  profile_strategy:
    alternating: [cheap_checkpoints, quality_gate]
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution
                .resolve_profile_name(0, ProfileName::LocalOnly),
            ProfileName::CheapCheckpoints
        );
        assert_eq!(
            cfg.execution
                .resolve_profile_name(1, ProfileName::LocalOnly),
            ProfileName::QualityGate
        );
        assert_eq!(
            cfg.execution
                .resolve_profile_name(2, ProfileName::LocalOnly),
            ProfileName::CheapCheckpoints
        );
    }

    #[test]
    fn resolve_profile_name_with_every_n_override() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: cheap_checkpoints
  profile_strategy:
    alternating: [cheap_checkpoints, quality_gate]
    every_n_cycles:
      - interval: 3
        profile: unblock_first
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        assert_eq!(
            cfg.execution
                .resolve_profile_name(3, ProfileName::LocalOnly),
            ProfileName::UnblockFirst
        );
    }

    #[test]
    fn resolve_profile_rules_with_mixin_fields() {
        let yml = r#"
version: 1
agents: {}
execution:
  profile: cheap_checkpoints
  profile_strategy:
    mixins:
      - from: unblock_first
        fields: [escalation]
  acceptance_criteria: []
  verification:
    commands: []
"#;

        let cfg: MachineConfig = serde_yaml::from_str(yml).unwrap();
        let rules = cfg
            .execution
            .resolve_profile_rules(0, ProfileName::CheapCheckpoints);

        assert_eq!(rules.default_agent, AgentPreference::LocalLlm);
        assert_eq!(rules.review_agent, Some(AgentPreference::Claude));
        let escalation = rules.escalation.expect("escalation should exist");
        assert_eq!(escalation.failure_threshold, 1);
        assert_eq!(escalation.escalate_to, AgentPreference::Codex);
    }

    #[test]
    fn resolve_profile_rules_with_scheduled_mixin() {
        let mut execution = ExecutionConfig::default();
        execution.profile = Some(ProfileName::CheapCheckpoints);
        execution
            .profile_strategy
            .mixins
            .push(super::ProfileMixinConfig {
                from: ProfileName::QualityGate,
                fields: vec![ProfileRuleField::Escalation],
                every_n_cycles: Some(2),
                offset: 1,
            });

        let rules0 = execution.resolve_profile_rules(0, ProfileName::LocalOnly);
        let rules1 = execution.resolve_profile_rules(1, ProfileName::LocalOnly);

        assert_eq!(
            rules0.escalation.expect("escalation exists").escalate_to,
            AgentPreference::Codex
        );
        assert_eq!(
            rules1.escalation.expect("escalation exists").escalate_to,
            AgentPreference::Claude
        );
    }
}
