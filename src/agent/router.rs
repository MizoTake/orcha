use std::collections::HashMap;

use crate::agent::backend::anthropic::AnthropicAgent;
use crate::agent::backend::codex::CodexAgent;
use crate::agent::backend::gemini::GeminiAgent;
use crate::agent::backend::local_cli::LocalCliAgent;
use crate::agent::backend::local_llm::LocalLlmAgent;
use crate::agent::{Agent, AgentKind};
use crate::config::AppConfig;
use crate::core::cycle::Phase;
use crate::core::gate::{self, GateDecision};
use crate::core::profile::{AgentPreference, ProfileRules};
use crate::machine_config::LocalLlmMode;

/// Routes agent requests to the appropriate backend based on profile and gates.
pub struct AgentRouter {
    agents: HashMap<AgentKind, Box<dyn Agent>>,
    profile_rules: ProfileRules,
}

/// Context for evaluating gates when selecting an agent.
pub struct GateContext {
    pub diff_content: Option<String>,
    pub diff_lines: usize,
    pub file_paths: Vec<String>,
    pub consecutive_verify_failures: u32,
}

impl Default for GateContext {
    fn default() -> Self {
        Self {
            diff_content: None,
            diff_lines: 0,
            file_paths: Vec::new(),
            consecutive_verify_failures: 0,
        }
    }
}

impl AgentRouter {
    pub fn new(config: &AppConfig, rules: &ProfileRules) -> anyhow::Result<Self> {
        let mut agents: HashMap<AgentKind, Box<dyn Agent>> = HashMap::new();

        // Local LLM backend selection
        let local_agent: Box<dyn Agent> = match config.local_llm_mode {
            LocalLlmMode::Http => Box::new(LocalLlmAgent::new(config)),
            LocalLlmMode::Cli => Box::new(LocalCliAgent::new(config)?),
        };
        agents.insert(AgentKind::LocalLlm, local_agent);

        // Optionally add paid agents
        if config.has_anthropic() {
            if let Ok(agent) = AnthropicAgent::new(config) {
                agents.insert(AgentKind::Claude, Box::new(agent));
            }
        }
        if config.has_gemini() {
            if let Ok(agent) = GeminiAgent::new(config) {
                agents.insert(AgentKind::Gemini, Box::new(agent));
            }
        }
        if config.has_openai() {
            if let Ok(agent) = CodexAgent::new(config) {
                agents.insert(AgentKind::Codex, Box::new(agent));
            }
        }

        Ok(Self {
            agents,
            profile_rules: rules.clone(),
        })
    }

    /// Select the appropriate agent for the current phase, considering gates.
    pub fn select(&self, phase: Phase, gate_ctx: &GateContext) -> &dyn Agent {
        // 1. Evaluate security gate
        if self.profile_rules.security_gate_enabled {
            let decision = gate::evaluate_security_gate(
                gate_ctx.diff_content.as_deref(),
                &gate_ctx.file_paths,
            );
            if let GateDecision::RequireAgent(pref) = decision {
                if let Some(agent) = self.get_by_preference(pref) {
                    return agent;
                }
            }
        }

        // 2. Evaluate unblock gate
        let unblock =
            gate::evaluate_unblock_gate(gate_ctx.consecutive_verify_failures, &self.profile_rules);
        if let GateDecision::RequireAgent(pref) = unblock {
            if let Some(agent) = self.get_by_preference(pref) {
                return agent;
            }
        }

        // 3. Check phase-specific rules
        if phase == Phase::Review {
            if let Some(review_pref) = self.profile_rules.review_agent {
                if let Some(agent) = self.get_by_preference(review_pref) {
                    return agent;
                }
            }
        }

        // 4. Size gate (recommendation only, don't force)
        if self.profile_rules.size_gate_enabled {
            let size = gate::evaluate_size_gate(gate_ctx.diff_lines);
            if let GateDecision::RecommendAgent(pref) = size {
                if let Some(agent) = self.get_by_preference(pref) {
                    // Log recommendation but still use it
                    return agent;
                }
            }
        }

        // 5. Fall back to default
        self.get_by_preference(self.profile_rules.default_agent)
            .expect("default agent (local_llm) must always be available")
    }

    /// Get the default agent (for phases that don't need gate checks).
    pub fn default_agent(&self) -> &dyn Agent {
        self.get_by_preference(self.profile_rules.default_agent)
            .expect("default agent must always be available")
    }

    fn get_by_preference(&self, pref: AgentPreference) -> Option<&dyn Agent> {
        let kind = match pref {
            AgentPreference::LocalLlm => AgentKind::LocalLlm,
            AgentPreference::Claude => AgentKind::Claude,
            AgentPreference::Gemini => AgentKind::Gemini,
            AgentPreference::Codex => AgentKind::Codex,
        };
        self.agents.get(&kind).map(|a| a.as_ref())
    }
}
