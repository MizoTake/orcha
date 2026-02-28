use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    pub owner: String,
    pub evidence: String,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Todo,
    Doing,
    Done,
    Blocked,
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskState::Todo => write!(f, "todo"),
            TaskState::Doing => write!(f, "doing"),
            TaskState::Done => write!(f, "done"),
            TaskState::Blocked => write!(f, "blocked"),
        }
    }
}

impl TaskState {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "todo" => Some(TaskState::Todo),
            "doing" => Some(TaskState::Doing),
            "done" => Some(TaskState::Done),
            "blocked" => Some(TaskState::Blocked),
            _ => None,
        }
    }
}

/// Parse a markdown task table into Task structs.
/// Expected format:
/// | ID | Title | State | Owner | Evidence | Notes |
/// |---|---|---|---|---|---|
/// | T1 | Do something | todo | local_llm | | |
pub fn parse_task_table(markdown: &str) -> anyhow::Result<Vec<Task>> {
    let mut tasks = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }

        let cols: Vec<&str> = trimmed
            .split('|')
            .map(|s| s.trim())
            .collect::<Vec<_>>();

        // Skip empty first/last from split, need at least 7 parts (empty + 6 cols + empty)
        if cols.len() < 8 {
            continue;
        }

        let id = cols[1].trim();
        let title = cols[2].trim();
        let state_str = cols[3].trim();
        let owner = cols[4].trim();
        let evidence = cols[5].trim();
        let notes = cols[6].trim();

        // Skip header row and separator row
        if id == "ID" || id.chars().all(|c| c == '-' || c == ' ') {
            continue;
        }

        let state = TaskState::from_str(state_str).ok_or_else(|| {
            crate::core::error::OrchaError::TaskTableParseError {
                line: i + 1,
                reason: format!("Unknown state: '{}'", state_str),
            }
        })?;

        tasks.push(Task {
            id: id.to_string(),
            title: title.to_string(),
            state,
            owner: owner.to_string(),
            evidence: evidence.to_string(),
            notes: notes.to_string(),
        });
    }

    Ok(tasks)
}

/// Render Task structs into a markdown table.
pub fn render_task_table(tasks: &[Task]) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | State | Owner | Evidence | Notes |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for t in tasks {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            t.id, t.title, t.state, t.owner, t.evidence, t.notes
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_table() {
        let table = "| ID | Title | State | Owner | Evidence | Notes |\n|---|---|---|---|---|---|\n";
        let tasks = parse_task_table(table).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn parse_basic_table() {
        let table = "| ID | Title | State | Owner | Evidence | Notes |\n|---|---|---|---|---|---|\n| T1 | Setup DB | done | local_llm | tests pass | |\n| T2 | Add auth | doing | local_llm | | WIP |\n";
        let tasks = parse_task_table(table).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "T1");
        assert_eq!(tasks[0].state, TaskState::Done);
        assert_eq!(tasks[1].id, "T2");
        assert_eq!(tasks[1].state, TaskState::Doing);
        assert_eq!(tasks[1].notes, "WIP");
    }

    #[test]
    fn render_roundtrip() {
        let tasks = vec![
            Task {
                id: "T1".into(),
                title: "Setup".into(),
                state: TaskState::Todo,
                owner: "local_llm".into(),
                evidence: "".into(),
                notes: "".into(),
            },
        ];
        let rendered = render_task_table(&tasks);
        let parsed = parse_task_table(&rendered).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "T1");
        assert_eq!(parsed[0].state, TaskState::Todo);
    }
}
