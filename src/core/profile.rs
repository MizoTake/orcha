use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileName {
    LocalOnly,
    CheapCheckpoints,
    QualityGate,
    UnblockFirst,
    OpencodeImplNoReview,
    OpencodeImplClaudeReview,
    OpencodeImplCodexReview,
    ClaudeImplOpencodeReview,
    CodexImplOpencodeReview,
    CodexReview,
}

impl ProfileName {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "local_only" => Some(ProfileName::LocalOnly),
            "cheap_checkpoints" => Some(ProfileName::CheapCheckpoints),
            "quality_gate" => Some(ProfileName::QualityGate),
            "unblock_first" => Some(ProfileName::UnblockFirst),
            "opencode_impl_no_review" => Some(ProfileName::OpencodeImplNoReview),
            "opencode_impl_claude_review" => Some(ProfileName::OpencodeImplClaudeReview),
            "opencode_impl_codex_review" => Some(ProfileName::OpencodeImplCodexReview),
            "claude_impl_opencode_review" => Some(ProfileName::ClaudeImplOpencodeReview),
            "codex_impl_opencode_review" => Some(ProfileName::CodexImplOpencodeReview),
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
            ProfileName::OpencodeImplNoReview,
            ProfileName::OpencodeImplClaudeReview,
            ProfileName::OpencodeImplCodexReview,
            ProfileName::ClaudeImplOpencodeReview,
            ProfileName::CodexImplOpencodeReview,
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
            ProfileName::OpencodeImplNoReview => write!(f, "opencode_impl_no_review"),
            ProfileName::OpencodeImplClaudeReview => {
                write!(f, "opencode_impl_claude_review")
            }
            ProfileName::OpencodeImplCodexReview => {
                write!(f, "opencode_impl_codex_review")
            }
            ProfileName::ClaudeImplOpencodeReview => {
                write!(f, "claude_impl_opencode_review")
            }
            ProfileName::CodexImplOpencodeReview => {
                write!(f, "codex_impl_opencode_review")
            }
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
            ProfileName::OpencodeImplNoReview => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: None,
                escalation: None,
                security_gate_enabled: false,
                size_gate_enabled: false,
            },
            ProfileName::OpencodeImplClaudeReview => ProfileRules {
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
            ProfileName::OpencodeImplCodexReview => ProfileRules {
                name,
                default_agent: AgentPreference::LocalLlm,
                review_agent: Some(AgentPreference::Codex),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Codex,
                    continued_failure_to: None,
                }),
                security_gate_enabled: false,
                size_gate_enabled: false,
            },
            ProfileName::ClaudeImplOpencodeReview => ProfileRules {
                name,
                default_agent: AgentPreference::Claude,
                review_agent: Some(AgentPreference::LocalLlm),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Claude,
                    continued_failure_to: None,
                }),
                security_gate_enabled: true,
                size_gate_enabled: true,
            },
            ProfileName::CodexImplOpencodeReview => ProfileRules {
                name,
                default_agent: AgentPreference::Codex,
                review_agent: Some(AgentPreference::LocalLlm),
                escalation: Some(EscalationRule {
                    failure_threshold: 2,
                    escalate_to: AgentPreference::Codex,
                    continued_failure_to: None,
                }),
                security_gate_enabled: false,
                size_gate_enabled: false,
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
        self.name != ProfileName::LocalOnly && self.name != ProfileName::OpencodeImplNoReview
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentPreference, ProfileName, ProfileRules};

    #[test]
    fn from_str_accepts_current_opencode_custom_profiles() {
        assert!(ProfileName::from_str("opencode_impl_no_review").is_some());
        assert!(ProfileName::from_str("opencode_impl_claude_review").is_some());
        assert!(ProfileName::from_str("opencode_impl_codex_review").is_some());
        assert!(ProfileName::from_str("claude_impl_opencode_review").is_some());
        assert!(ProfileName::from_str("codex_impl_opencode_review").is_some());
    }

    #[test]
    fn from_str_rejects_legacy_opencode_profile_names() {
        assert!(ProfileName::from_str("opencode_only").is_none());
        assert!(ProfileName::from_str("opencode_claude").is_none());
        assert!(ProfileName::from_str("opencode_codex").is_none());
        assert!(ProfileName::from_str("opencode_claude_swapped").is_none());
        assert!(ProfileName::from_str("opencode_codex_swapped").is_none());
    }

    #[test]
    fn opencode_impl_claude_review_prefers_claude_for_review_and_escalation() {
        let profile =
            ProfileName::from_str("opencode_impl_claude_review").expect("profile exists");
        let rules = ProfileRules::from_name(profile);

        assert_eq!(rules.default_agent, AgentPreference::LocalLlm);
        assert_eq!(rules.review_agent, Some(AgentPreference::Claude));
        let escalation = rules.escalation.expect("escalation exists");
        assert_eq!(escalation.failure_threshold, 2);
        assert_eq!(escalation.escalate_to, AgentPreference::Claude);
        assert!(rules.security_gate_enabled);
        assert!(rules.size_gate_enabled);
    }

    #[test]
    fn opencode_impl_codex_review_uses_codex_without_claude_gates() {
        let profile =
            ProfileName::from_str("opencode_impl_codex_review").expect("profile exists");
        let rules = ProfileRules::from_name(profile);

        assert_eq!(rules.default_agent, AgentPreference::LocalLlm);
        assert_eq!(rules.review_agent, Some(AgentPreference::Codex));
        let escalation = rules.escalation.expect("escalation exists");
        assert_eq!(escalation.failure_threshold, 2);
        assert_eq!(escalation.escalate_to, AgentPreference::Codex);
        assert!(!rules.security_gate_enabled);
        assert!(!rules.size_gate_enabled);
    }

    #[test]
    fn claude_impl_opencode_review_uses_claude_for_default_and_local_for_review() {
        let profile =
            ProfileName::from_str("claude_impl_opencode_review").expect("profile exists");
        let rules = ProfileRules::from_name(profile);

        assert_eq!(rules.default_agent, AgentPreference::Claude);
        assert_eq!(rules.review_agent, Some(AgentPreference::LocalLlm));
        let escalation = rules.escalation.expect("escalation exists");
        assert_eq!(escalation.failure_threshold, 2);
        assert_eq!(escalation.escalate_to, AgentPreference::Claude);
        assert!(rules.security_gate_enabled);
        assert!(rules.size_gate_enabled);
    }

    #[test]
    fn codex_impl_opencode_review_uses_codex_for_default_and_local_for_review() {
        let profile =
            ProfileName::from_str("codex_impl_opencode_review").expect("profile exists");
        let rules = ProfileRules::from_name(profile);

        assert_eq!(rules.default_agent, AgentPreference::Codex);
        assert_eq!(rules.review_agent, Some(AgentPreference::LocalLlm));
        let escalation = rules.escalation.expect("escalation exists");
        assert_eq!(escalation.failure_threshold, 2);
        assert_eq!(escalation.escalate_to, AgentPreference::Codex);
        assert!(!rules.security_gate_enabled);
        assert!(!rules.size_gate_enabled);
    }
}
