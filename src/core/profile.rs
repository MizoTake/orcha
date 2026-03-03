use std::fmt;
use std::path::Path;

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

pub fn load_custom_profile_rules(
    orch_dir: &Path,
    profile_name: &str,
    fallback: ProfileName,
) -> anyhow::Result<Option<ProfileRules>> {
    let key = normalize_profile_key(profile_name);
    let profile_path = orch_dir.join("profiles").join(format!("{key}.md"));
    if !profile_path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&profile_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read custom profile file {}: {}",
            profile_path.display(),
            e
        )
    })?;

    Ok(Some(parse_custom_profile_rules(&raw, fallback)))
}

fn parse_custom_profile_rules(raw: &str, fallback: ProfileName) -> ProfileRules {
    let mut rules = ProfileRules::from_name(fallback);

    if let Some(value) = extract_rule_value(raw, "Default agent") {
        if let Some(agent) = parse_agent_preference(&value) {
            rules.default_agent = agent;
        }
    }

    if let Some(value) = extract_rule_value(raw, "Review agent") {
        let lower = value.to_ascii_lowercase();
        if lower.contains("none") || lower.contains("disabled") {
            rules.review_agent = None;
        } else if let Some(agent) = parse_agent_preference(&value) {
            rules.review_agent = Some(agent);
        }
    }

    if let Some(value) = extract_rule_value(raw, "Escalation") {
        rules.escalation = parse_escalation_rule(&value);
    }

    if let Some(value) = extract_rule_value(raw, "Security gate") {
        if let Some(enabled) = parse_enabled_disabled(&value) {
            rules.security_gate_enabled = enabled;
        }
    }

    if let Some(value) = extract_rule_value(raw, "Size gate") {
        if let Some(enabled) = parse_enabled_disabled(&value) {
            rules.size_gate_enabled = enabled;
        }
    }

    rules
}

fn extract_rule_value(raw: &str, label: &str) -> Option<String> {
    let prefix = format!("- **{label}**:");
    raw.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(|s| s.trim().to_string()))
}

fn parse_enabled_disabled(value: &str) -> Option<bool> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.starts_with("enabled") {
        Some(true)
    } else if lower.starts_with("disabled") {
        Some(false)
    } else {
        None
    }
}

fn parse_escalation_rule(value: &str) -> Option<EscalationRule> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.starts_with("none") || lower.starts_with("disabled") {
        return None;
    }

    let normalized = value.replace("->", "→");
    let parts: Vec<&str> = normalized.split('→').collect();
    if parts.len() < 2 {
        return None;
    }

    let threshold = extract_first_u32(parts[0]).unwrap_or(1);
    let escalate_to = parse_agent_preference(parts[1])?;
    let continued_failure_to = if parts.len() >= 3 {
        parse_agent_preference(parts[2])
    } else {
        None
    };

    Some(EscalationRule {
        failure_threshold: threshold,
        escalate_to,
        continued_failure_to,
    })
}

fn extract_first_u32(text: &str) -> Option<u32> {
    let mut digits = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }

    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

fn parse_agent_preference(value: &str) -> Option<AgentPreference> {
    let normalized = normalize_profile_key(
        &value
            .replace('(', " ")
            .replace(')', " ")
            .replace(',', " ")
            .replace(':', " "),
    );
    for token in normalized.split_whitespace() {
        match token {
            "local_llm" | "local" | "opencode" => return Some(AgentPreference::LocalLlm),
            "claude" | "claude_code" | "claudecode" => return Some(AgentPreference::Claude),
            "gemini" => return Some(AgentPreference::Gemini),
            "codex" => return Some(AgentPreference::Codex),
            _ => {}
        }
    }
    None
}

fn normalize_profile_key(raw: &str) -> String {
    raw.trim().to_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

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

    #[test]
    fn load_custom_profile_rules_parses_markdown_rules() {
        let dir = tempdir().expect("tempdir");
        let profiles_dir = dir.path().join("profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        fs::write(
            profiles_dir.join("claude_impl_no_review.md"),
            r#"# Profile: claude_impl_no_review

## Rules

- **Default agent**: claude
- **Review agent**: none
- **Escalation**: None
- **Security gate**: Disabled
- **Size gate**: Disabled
"#,
        )
        .expect("write profile");

        let parsed = super::load_custom_profile_rules(
            dir.path(),
            "claude_impl_no_review",
            ProfileName::CheapCheckpoints,
        )
        .expect("parse custom profile")
        .expect("custom profile exists");

        assert_eq!(parsed.default_agent, AgentPreference::Claude);
        assert_eq!(parsed.review_agent, None);
        assert!(parsed.escalation.is_none());
        assert!(!parsed.security_gate_enabled);
        assert!(!parsed.size_gate_enabled);
    }

    #[test]
    fn load_custom_profile_rules_returns_none_when_file_missing() {
        let dir = tempdir().expect("tempdir");
        let parsed = super::load_custom_profile_rules(
            dir.path(),
            "missing_custom_profile",
            ProfileName::CheapCheckpoints,
        )
        .expect("load should succeed");
        assert!(parsed.is_none());
    }

    // ── extract_first_u32 ────────────────────────────────────────────────────

    #[test]
    fn extract_first_u32_parses_leading_number() {
        assert_eq!(super::extract_first_u32("3 failures"), Some(3));
    }

    #[test]
    fn extract_first_u32_parses_number_surrounded_by_text() {
        assert_eq!(super::extract_first_u32("after 5 retries"), Some(5));
    }

    #[test]
    fn extract_first_u32_returns_none_when_no_digit() {
        assert_eq!(super::extract_first_u32("no number here"), None);
    }

    #[test]
    fn extract_first_u32_returns_none_for_empty_input() {
        assert_eq!(super::extract_first_u32(""), None);
    }

    // ── parse_agent_preference ───────────────────────────────────────────────

    #[test]
    fn parse_agent_preference_recognizes_claude_aliases() {
        assert_eq!(
            super::parse_agent_preference("claude"),
            Some(AgentPreference::Claude)
        );
        assert_eq!(
            super::parse_agent_preference("claude_code"),
            Some(AgentPreference::Claude)
        );
    }

    #[test]
    fn parse_agent_preference_recognizes_local_llm_aliases() {
        assert_eq!(
            super::parse_agent_preference("local_llm"),
            Some(AgentPreference::LocalLlm)
        );
        assert_eq!(
            super::parse_agent_preference("opencode"),
            Some(AgentPreference::LocalLlm)
        );
    }

    #[test]
    fn parse_agent_preference_recognizes_codex_and_gemini() {
        assert_eq!(
            super::parse_agent_preference("codex"),
            Some(AgentPreference::Codex)
        );
        assert_eq!(
            super::parse_agent_preference("gemini"),
            Some(AgentPreference::Gemini)
        );
    }

    #[test]
    fn parse_agent_preference_returns_none_for_unknown() {
        assert_eq!(super::parse_agent_preference("unknown-agent"), None);
        assert_eq!(super::parse_agent_preference(""), None);
    }

    // ── parse_escalation_rule ────────────────────────────────────────────────

    #[test]
    fn parse_escalation_rule_arrow_notation() {
        let rule = super::parse_escalation_rule("2 failures -> codex")
            .expect("should parse");
        assert_eq!(rule.failure_threshold, 2);
        assert_eq!(rule.escalate_to, AgentPreference::Codex);
        assert!(rule.continued_failure_to.is_none());
    }

    #[test]
    fn parse_escalation_rule_unicode_arrow_notation() {
        let rule = super::parse_escalation_rule("3 failures → claude")
            .expect("should parse");
        assert_eq!(rule.failure_threshold, 3);
        assert_eq!(rule.escalate_to, AgentPreference::Claude);
    }

    #[test]
    fn parse_escalation_rule_three_part_with_continued_failure() {
        let rule = super::parse_escalation_rule("1 → codex → claude")
            .expect("should parse");
        assert_eq!(rule.failure_threshold, 1);
        assert_eq!(rule.escalate_to, AgentPreference::Codex);
        assert_eq!(rule.continued_failure_to, Some(AgentPreference::Claude));
    }

    #[test]
    fn parse_escalation_rule_none_returns_none() {
        assert!(super::parse_escalation_rule("None").is_none());
        assert!(super::parse_escalation_rule("disabled").is_none());
    }

    #[test]
    fn parse_escalation_rule_missing_arrow_returns_none() {
        assert!(super::parse_escalation_rule("codex").is_none());
    }

    // ── is_paid_available ────────────────────────────────────────────────────

    #[test]
    fn is_paid_available_false_for_local_only_and_opencode_no_review() {
        assert!(!ProfileRules::from_name(ProfileName::LocalOnly).is_paid_available());
        assert!(!ProfileRules::from_name(ProfileName::OpencodeImplNoReview).is_paid_available());
    }

    #[test]
    fn is_paid_available_true_for_paid_profiles() {
        for profile in &[
            ProfileName::CheapCheckpoints,
            ProfileName::QualityGate,
            ProfileName::UnblockFirst,
            ProfileName::OpencodeImplClaudeReview,
        ] {
            assert!(
                ProfileRules::from_name(*profile).is_paid_available(),
                "{profile} should have paid available"
            );
        }
    }

    // ── ProfileName::Display ─────────────────────────────────────────────────

    #[test]
    fn profile_name_display_roundtrips_through_from_str() {
        for name in ProfileName::all() {
            let s = name.to_string();
            let parsed = ProfileName::from_str(&s);
            assert_eq!(parsed, Some(*name), "roundtrip failed for {s}");
        }
    }

    #[test]
    fn load_custom_profile_rules_can_override_known_profile_from_file() {
        let dir = tempdir().expect("tempdir");
        let profiles_dir = dir.path().join("profiles");
        fs::create_dir_all(&profiles_dir).expect("create profiles dir");
        fs::write(
            profiles_dir.join("cheap_checkpoints.md"),
            r#"# Profile: cheap_checkpoints

## Rules

- **Default agent**: codex
- **Review agent**: none
- **Escalation**: None
- **Security gate**: Disabled
- **Size gate**: Disabled
"#,
        )
        .expect("write profile");

        let parsed = super::load_custom_profile_rules(
            dir.path(),
            "cheap_checkpoints",
            ProfileName::CheapCheckpoints,
        )
        .expect("parse profile from file")
        .expect("profile file should exist");

        assert_eq!(parsed.default_agent, AgentPreference::Codex);
        assert_eq!(parsed.review_agent, None);
        assert!(parsed.escalation.is_none());
        assert!(!parsed.security_gate_enabled);
        assert!(!parsed.size_gate_enabled);
    }
}
