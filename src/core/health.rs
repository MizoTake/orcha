use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core::task::{Task, TaskState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Green,
    Yellow,
    Red,
}

impl fmt::Display for Health {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Health::Green => write!(f, "green"),
            Health::Yellow => write!(f, "yellow"),
            Health::Red => write!(f, "red"),
        }
    }
}

impl Health {
    /// Derive health from current state.
    pub fn evaluate(tasks: &[Task], verify_passed: Option<bool>, has_review_issues: bool) -> Self {
        // Red: verify failed or any task blocked
        if verify_passed == Some(false) {
            return Health::Red;
        }
        if tasks.iter().any(|t| t.state == TaskState::Blocked) {
            return Health::Red;
        }

        // Yellow: review found issues
        if has_review_issues {
            return Health::Yellow;
        }

        Health::Green
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_when_all_ok() {
        let tasks = vec![Task {
            id: "T1".into(),
            title: "x".into(),
            state: TaskState::Done,
            owner: "".into(),
            evidence: "".into(),
            notes: "".into(),
        }];
        assert_eq!(Health::evaluate(&tasks, Some(true), false), Health::Green);
    }

    #[test]
    fn red_when_verify_failed() {
        assert_eq!(Health::evaluate(&[], Some(false), false), Health::Red);
    }

    #[test]
    fn red_when_task_blocked() {
        let tasks = vec![Task {
            id: "T1".into(),
            title: "x".into(),
            state: TaskState::Blocked,
            owner: "".into(),
            evidence: "".into(),
            notes: "".into(),
        }];
        assert_eq!(Health::evaluate(&tasks, None, false), Health::Red);
    }

    #[test]
    fn yellow_when_review_issues() {
        assert_eq!(Health::evaluate(&[], Some(true), true), Health::Yellow);
    }
}
