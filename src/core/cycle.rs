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
}

impl fmt::Display for StopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StopReason::MaxCyclesReached => write!(f, "Maximum cycles (5) reached"),
            StopReason::RepeatedFailureNoPaid => {
                write!(f, "Same failure repeated and no paid model available")
            }
            StopReason::LocalOnlyStuck => {
                write!(f, "Local-only profile and stuck on failure")
            }
        }
    }
}

pub const MAX_CYCLES: u32 = 5;

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
}
