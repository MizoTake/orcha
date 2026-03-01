use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileName {
    LocalOnly,
    CheapCheckpoints,
    QualityGate,
    UnblockFirst,
    CodexReview,
}

impl ProfileName {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "local_only" => Some(ProfileName::LocalOnly),
            "cheap_checkpoints" => Some(ProfileName::CheapCheckpoints),
            "quality_gate" => Some(ProfileName::QualityGate),
            "unblock_first" => Some(ProfileName::UnblockFirst),
            "codex_review" => Some(ProfileName::CodexReview),
            _ => None,
        }
    }

    pub fn all() -> &'static [ProfileName] {
        &[
            ProfileName::LocalOnly,
            ProfileName::CheapCheckpoints,
            ProfileName::QualityGate,
            ProfileName::UnblockFirst,
            ProfileName::CodexReview,
        ]
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileName::LocalOnly => write!(f, "local_only"),
            ProfileName::CheapCheckpoints => write!(f, "cheap_checkpoints"),
            ProfileName::QualityGate => write!(f, "quality_gate"),
            ProfileName::UnblockFirst => write!(f, "unblock_first"),
            ProfileName::CodexReview => write!(f, "codex_review"),
        }
    }
}

/// Resolved rules for the current profile.
#[derive(Debug, Clone)]
pub struct ProfileRules {
    pub name: ProfileName,
    pub default_agent: AgentPreference,
    pub review_agent: Option<AgentPreference>,
    pub escalation: Option<EscalationRule>,
    pub security_gate_enabled: bool,
    pub size_gate_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPreference {
    LocalLlm,
    Claude,
    Gemini,
    Codex,
}

impl fmt::Display for AgentPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentPreference::LocalLlm => write!(f, "local_llm"),
            AgentPreference::Claude => write!(f, "claude"),
            AgentPreference::Gemini => write!(f, "gemini"),
            AgentPreference::Codex => write!(f, "codex"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EscalationRule {
    pub failure_threshold: u32,
    pub escalate_to: AgentPreference,
    pub continued_failure_to: Option<AgentPreference>,
}

impl ProfileRules {
    pub fn from_name(name: ProfileName) -> Self {
        match name {
            ProfileName::LocalOnly => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: None,
                escalation: None,
                security_gate_enabled: false,
                size_gate_enabled: false,
            },
            ProfileName::CheapCheckpoints => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: Some(AgentPreference::Claude),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Codex,
                    continued_failure_to: None,
                }),
                security_gate_enabled: true,
                size_gate_enabled: true,
            },
            ProfileName::QualityGate => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: Some(AgentPreference::Claude),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Claude,
                    continued_failure_to: None,
                }),
                security_gate_enabled: true,
                size_gate_enabled: true,
            },
            ProfileName::UnblockFirst => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: None,
                escalation: Some(EscalationRule {
                    failure_threshold: 1,
                    escalate_to: AgentPreference::Codex,
                    continued_failure_to: Some(AgentPreference::Claude),
                }),
                security_gate_enabled: true,
                size_gate_enabled: true,
            },
            ProfileName::CodexReview => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: Some(AgentPreference::Codex),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Claude,
                    continued_failure_to: None,
                }),
                security_gate_enabled: true,
                size_gate_enabled: true,
            },
        }
    }

    pub fn is_paid_available(&self) -> bool {
        self.name != ProfileName::LocalOnly
    }
}
