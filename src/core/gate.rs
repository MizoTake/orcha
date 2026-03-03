use regex::Regex;
use std::sync::LazyLock;

use crate::core::profile::{AgentPreference, ProfileRules};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    UseDefault,
    RequireAgent(AgentPreference),
    RecommendAgent(AgentPreference),
}

static SECURITY_KEYWORDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(auth|crypto|security|public[\s_-]?api)\b").unwrap());

/// Security Gate: check for auth/crypto/security keywords in diff or file paths.
/// If found, require Claude review.
pub fn evaluate_security_gate(diff_content: Option<&str>, file_paths: &[String]) -> GateDecision {
    // Check file paths
    for path in file_paths {
        if SECURITY_KEYWORDS.is_match(path) {
            return GateDecision::RequireAgent(AgentPreference::Claude);
        }
    }

    // Check diff content
    if let Some(diff) = diff_content {
        if SECURITY_KEYWORDS.is_match(diff) {
            return GateDecision::RequireAgent(AgentPreference::Claude);
        }
    }

    GateDecision::UseDefault
}

/// Unblock Gate: if verify has failed >= threshold times, escalate.
pub fn evaluate_unblock_gate(consecutive_failures: u32, rules: &ProfileRules) -> GateDecision {
    if let Some(ref esc) = rules.escalation {
        if consecutive_failures >= esc.failure_threshold {
            // Check if continued failure threshold is met
            if let Some(ref continued) = esc.continued_failure_to {
                if consecutive_failures >= esc.failure_threshold + 1 {
                    return GateDecision::RequireAgent(*continued);
                }
            }
            return GateDecision::RequireAgent(esc.escalate_to);
        }
    }
    GateDecision::UseDefault
}

/// Size Gate: if diff is large (> 400 lines), recommend paid review.
pub fn evaluate_size_gate(diff_lines: usize) -> GateDecision {
    if diff_lines > 400 {
        GateDecision::RecommendAgent(AgentPreference::Claude)
    } else {
        GateDecision::UseDefault
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::profile::ProfileName;

    #[test]
    fn security_gate_detects_auth_in_path() {
        let paths = vec!["src/auth/middleware.rs".to_string()];
        assert_eq!(
            evaluate_security_gate(None, &paths),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn security_gate_detects_crypto_in_diff() {
        let decision = evaluate_security_gate(Some("added crypto hashing"), &[]);
        assert_eq!(
            decision,
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn security_gate_passes_normal_code() {
        let decision =
            evaluate_security_gate(Some("added button component"), &["src/ui.rs".into()]);
        assert_eq!(decision, GateDecision::UseDefault);
    }

    #[test]
    fn unblock_gate_triggers_after_threshold() {
        let rules = ProfileRules::from_name(ProfileName::CheapCheckpoints);
        let decision = evaluate_unblock_gate(2, &rules);
        assert_eq!(decision, GateDecision::RequireAgent(AgentPreference::Codex));
    }

    #[test]
    fn unblock_gate_no_trigger_below_threshold() {
        let rules = ProfileRules::from_name(ProfileName::CheapCheckpoints);
        let decision = evaluate_unblock_gate(1, &rules);
        assert_eq!(decision, GateDecision::UseDefault);
    }

    #[test]
    fn size_gate_triggers_above_400() {
        assert_eq!(
            evaluate_size_gate(450),
            GateDecision::RecommendAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn size_gate_no_trigger_below_400() {
        assert_eq!(evaluate_size_gate(200), GateDecision::UseDefault);
    }

    #[test]
    fn size_gate_does_not_trigger_at_exactly_400() {
        assert_eq!(evaluate_size_gate(400), GateDecision::UseDefault);
    }

    #[test]
    fn size_gate_triggers_at_401() {
        assert_eq!(
            evaluate_size_gate(401),
            GateDecision::RecommendAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn security_gate_is_case_insensitive_for_keywords() {
        // "AUTH", "CRYPTO", etc. should still trigger
        assert_eq!(
            evaluate_security_gate(Some("uses AUTH tokens"), &[]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
        assert_eq!(
            evaluate_security_gate(Some("CRYPTO library"), &[]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
        assert_eq!(
            evaluate_security_gate(None, &["src/SECURITY.rs".to_string()]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn security_gate_triggers_on_public_api_keyword() {
        assert_eq!(
            evaluate_security_gate(Some("public_api endpoint"), &[]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
        assert_eq!(
            evaluate_security_gate(Some("public api key"), &[]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn security_gate_requires_whole_word_match() {
        // The regex uses \b so "authenticate" does NOT match "auth" (no word boundary after it).
        assert_eq!(
            evaluate_security_gate(Some("authenticate user"), &[]),
            GateDecision::UseDefault
        );
        // But "auth token" IS a whole-word match and should trigger.
        assert_eq!(
            evaluate_security_gate(Some("auth token"), &[]),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn unblock_gate_uses_default_when_no_escalation_rules() {
        let rules = ProfileRules::from_name(ProfileName::LocalOnly);
        assert!(rules.escalation.is_none());
        assert_eq!(evaluate_unblock_gate(100, &rules), GateDecision::UseDefault);
    }

    #[test]
    fn unblock_gate_escalates_to_continued_agent_after_extra_failure() {
        let rules = ProfileRules::from_name(ProfileName::UnblockFirst);
        // failure_threshold = 1; continued_failure_to = Some(Claude)
        // At exactly threshold → escalate_to (Codex)
        assert_eq!(
            evaluate_unblock_gate(1, &rules),
            GateDecision::RequireAgent(AgentPreference::Codex)
        );
        // One above threshold → continued_failure_to (Claude)
        assert_eq!(
            evaluate_unblock_gate(2, &rules),
            GateDecision::RequireAgent(AgentPreference::Claude)
        );
    }

    #[test]
    fn unblock_gate_no_continued_failure_agent_stays_at_escalate_to() {
        // CheapCheckpoints has no continued_failure_to
        let rules = ProfileRules::from_name(ProfileName::CheapCheckpoints);
        assert_eq!(
            evaluate_unblock_gate(5, &rules),
            GateDecision::RequireAgent(AgentPreference::Codex)
        );
    }
}
