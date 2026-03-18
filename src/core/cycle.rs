use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Briefing,
    Plan,
    Impl,
    Review,
    Fix,
    Verify,
    Decide,
}

impl Phase {
    pub fn next(self) -> Option<Phase> {
        match self {
            Phase::Briefing => Some(Phase::Plan),
            Phase::Plan => Some(Phase::Impl),
            Phase::Impl => Some(Phase::Review),
            Phase::Review => Some(Phase::Fix),
            Phase::Fix => Some(Phase::Verify),
            Phase::Verify => Some(Phase::Decide),
            Phase::Decide => None,
        }
    }

    pub fn role_name(&self) -> &'static str {
        match self {
            Phase::Briefing => "scribe",
            Phase::Plan => "planner",
            Phase::Impl => "implementer",
            Phase::Review => "reviewer",
            Phase::Fix => "implementer",
            Phase::Verify => "verifier",
            Phase::Decide => "planner",
        }
    }

    pub fn all() -> &'static [Phase] {
        &[
            Phase::Briefing,
            Phase::Plan,
            Phase::Impl,
            Phase::Review,
            Phase::Fix,
            Phase::Verify,
            Phase::Decide,
        ]
    }

    pub fn position(self) -> usize {
        match self {
            Phase::Briefing => 1,
            Phase::Plan => 2,
            Phase::Impl => 3,
            Phase::Review => 4,
            Phase::Fix => 5,
            Phase::Verify => 6,
            Phase::Decide => 7,
        }
    }

    pub fn total() -> usize {
        Self::all().len()
    }

    pub fn gauge(self) -> String {
        let done = self.position();
        let total = Self::total();
        format!("[{}{}]", "#".repeat(done), "-".repeat(total - done))
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Phase::Briefing => "briefing",
            Phase::Plan => "plan",
            Phase::Impl => "impl",
            Phase::Review => "review",
            Phase::Fix => "fix",
            Phase::Verify => "verify",
            Phase::Decide => "decide",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleDecision {
    NextPhase,
    NextCycle,
    Done,
    Blocked(StopReason),
    Escalate(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    MaxCyclesReached,
    RepeatedFailureNoPaid,
    LocalOnlyStuck,
    VerificationNotConfigured,
    BlockedTasksRequireIntervention,
    NoTasksFound,
}

impl fmt::Display for StopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StopReason::MaxCyclesReached => write!(f, "Maximum cycles reached"),
            StopReason::RepeatedFailureNoPaid => {
                write!(f, "Same failure repeated and no paid model available")
            }
            StopReason::LocalOnlyStuck => {
                write!(f, "Local-only profile and stuck on failure")
            }
            StopReason::VerificationNotConfigured => {
                write!(f, "Verification commands are not configured")
            }
            StopReason::BlockedTasksRequireIntervention => {
                write!(f, "Blocked tasks require human intervention")
            }
            StopReason::NoTasksFound => write!(f, "No task files found in tasks/todo — please add markdown files"),
        }
    }
}

pub const MAX_CYCLES: u32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_progression() {
        assert_eq!(Phase::Briefing.next(), Some(Phase::Plan));
        assert_eq!(Phase::Plan.next(), Some(Phase::Impl));
        assert_eq!(Phase::Impl.next(), Some(Phase::Review));
        assert_eq!(Phase::Review.next(), Some(Phase::Fix));
        assert_eq!(Phase::Fix.next(), Some(Phase::Verify));
        assert_eq!(Phase::Verify.next(), Some(Phase::Decide));
        assert_eq!(Phase::Decide.next(), None);
    }

    #[test]
    fn phase_role_names() {
        assert_eq!(Phase::Briefing.role_name(), "scribe");
        assert_eq!(Phase::Plan.role_name(), "planner");
        assert_eq!(Phase::Impl.role_name(), "implementer");
        assert_eq!(Phase::Review.role_name(), "reviewer");
        assert_eq!(Phase::Fix.role_name(), "implementer");
        assert_eq!(Phase::Verify.role_name(), "verifier");
        assert_eq!(Phase::Decide.role_name(), "planner");
    }

    #[test]
    fn phase_position_and_gauge() {
        assert_eq!(Phase::Briefing.position(), 1);
        assert_eq!(Phase::Decide.position(), 7);
        assert_eq!(Phase::total(), 7);
        assert_eq!(Phase::Briefing.gauge(), "[#------]");
        assert_eq!(Phase::Review.gauge(), "[####---]");
        assert_eq!(Phase::Decide.gauge(), "[#######]");
    }

    #[test]
    fn phase_display_matches_serde_rename() {
        assert_eq!(Phase::Briefing.to_string(), "briefing");
        assert_eq!(Phase::Plan.to_string(), "plan");
        assert_eq!(Phase::Impl.to_string(), "impl");
        assert_eq!(Phase::Review.to_string(), "review");
        assert_eq!(Phase::Fix.to_string(), "fix");
        assert_eq!(Phase::Verify.to_string(), "verify");
        assert_eq!(Phase::Decide.to_string(), "decide");
    }

    #[test]
    fn phase_all_returns_all_seven_phases() {
        let all = Phase::all();
        assert_eq!(all.len(), 7);
        assert_eq!(all[0], Phase::Briefing);
        assert_eq!(all[6], Phase::Decide);
    }

    #[test]
    fn stop_reason_display_messages_are_informative() {
        assert!(StopReason::MaxCyclesReached.to_string().contains("Maximum"));
        assert!(StopReason::RepeatedFailureNoPaid.to_string().contains("paid"));
        assert!(StopReason::LocalOnlyStuck.to_string().contains("stuck"));
    }

    #[test]
    fn cycle_decision_variants_are_distinct() {
        let decisions = [
            CycleDecision::NextPhase,
            CycleDecision::NextCycle,
            CycleDecision::Done,
            CycleDecision::Blocked(StopReason::MaxCyclesReached),
            CycleDecision::Escalate("needs human".to_string()),
        ];
        // Each variant should only equal itself.
        assert_eq!(decisions[0], CycleDecision::NextPhase);
        assert_ne!(decisions[0], CycleDecision::NextCycle);
        assert_eq!(
            decisions[3],
            CycleDecision::Blocked(StopReason::MaxCyclesReached)
        );
        assert_ne!(
            decisions[3],
            CycleDecision::Blocked(StopReason::LocalOnlyStuck)
        );
    }
}
