use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::cycle::Phase;
use crate::core::profile::ProfileName;
use crate::core::task::{parse_task_table, render_task_table, Task};
use crate::markdown::frontmatter::{self, Document};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusFrontmatter {
    pub run_id: String,
    pub profile: ProfileName,
    pub cycle: u32,
    pub phase: Phase,
    pub last_update: String,
    pub budget: Budget,
    pub locks: Locks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub paid_calls_used: u32,
    pub paid_calls_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locks {
    pub writer: Option<String>,
    pub active_task: Option<String>,
}

/// Full parsed status.md.
#[derive(Debug, Clone)]
pub struct StatusFile {
    pub frontmatter: StatusFrontmatter,
    pub content: String,
}

impl StatusFile {
    /// Load status.md from disk.
    pub async fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = tokio::fs::read_to_string(path).await?;
        Self::from_str(&raw)
    }

    /// Parse from string.
    pub fn from_str(input: &str) -> anyhow::Result<Self> {
        let doc: Document<StatusFrontmatter> = frontmatter::parse(input)?;
        Ok(StatusFile {
            frontmatter: doc.frontmatter,
            content: doc.content,
        })
    }

    /// Save status.md to disk.
    pub async fn save(&self, path: &Path) -> anyhow::Result<()> {
        let doc = Document {
            frontmatter: self.frontmatter.clone(),
            content: self.content.clone(),
        };
        let output = frontmatter::serialize(&doc)?;
        tokio::fs::write(path, output).await?;
        Ok(())
    }

    /// Extract tasks from the task table in the content.
    pub fn tasks(&self) -> anyhow::Result<Vec<Task>> {
        parse_task_table(&self.content)
    }

    /// Update a task in the content's task table.
    pub fn update_task(&mut self, updated: &Task) -> anyhow::Result<()> {
        let mut tasks = self.tasks()?;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == updated.id) {
            *t = updated.clone();
        }
        self.replace_task_table(&tasks);
        Ok(())
    }

    /// Replace the task table section in the content.
    pub fn replace_task_table(&mut self, tasks: &[Task]) {
        let new_table = render_task_table(tasks);

        // Find and replace the existing task table
        if let Some(start) = self.content.find("| ID |") {
            // Find the end of the table (next blank line or section header)
            let rest = &self.content[start..];
            let end = rest
                .find("\n\n")
                .map(|p| start + p)
                .unwrap_or(self.content.len());
            self.content = format!(
                "{}{}{}",
                &self.content[..start],
                new_table.trim_end(),
                &self.content[end..]
            );
        }
    }

    /// Advance to the next phase within the current cycle.
    pub fn advance_phase(&mut self) {
        if let Some(next) = self.frontmatter.phase.next() {
            self.frontmatter.phase = next;
        }
        self.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    }

    /// Start a new cycle (back to Briefing).
    pub fn start_new_cycle(&mut self) {
        self.frontmatter.cycle += 1;
        self.frontmatter.phase = Phase::Briefing;
        self.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> &'static str {
        "---\nrun_id: test-001\nprofile: cheap_checkpoints\ncycle: 1\nphase: plan\nlast_update: '2025-01-01T00:00:00Z'\nbudget:\n  paid_calls_used: 0\n  paid_calls_limit: 10\nlocks:\n  writer: null\n  active_task: null\n---\n\n## Goal\n\nBuild the thing.\n\n## Task Table\n\n| ID | Title | State | Owner | Evidence | Notes |\n|---|---|---|---|---|---|\n| T1 | Setup | done | local_llm | ok | |\n| T2 | Build | todo | | | |\n"
    }

    #[test]
    fn parse_status() {
        let status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.run_id, "test-001");
        assert_eq!(status.frontmatter.profile, ProfileName::CheapCheckpoints);
        assert_eq!(status.frontmatter.cycle, 1);
        assert_eq!(status.frontmatter.phase, Phase::Plan);
    }

    #[test]
    fn extract_tasks() {
        let status = StatusFile::from_str(sample_status()).unwrap();
        let tasks = status.tasks().unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "T1");
        assert_eq!(tasks[1].id, "T2");
    }

    #[test]
    fn advance_phase() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.phase, Phase::Plan);
        status.advance_phase();
        assert_eq!(status.frontmatter.phase, Phase::Impl);
    }

    #[test]
    fn start_new_cycle_increments_cycle_and_resets_to_briefing() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        assert_eq!(status.frontmatter.cycle, 1);
        assert_eq!(status.frontmatter.phase, Phase::Plan);

        status.start_new_cycle();

        assert_eq!(status.frontmatter.cycle, 2);
        assert_eq!(status.frontmatter.phase, Phase::Briefing);
    }

    #[test]
    fn start_new_cycle_increments_monotonically() {
        let mut status = StatusFile::from_str(sample_status()).unwrap();
        status.start_new_cycle();
        status.start_new_cycle();
        assert_eq!(status.frontmatter.cycle, 3);
    }

    #[test]
    fn replace_task_table_updates_existing_table() {
        use crate::core::task::{Task, TaskState};

        let mut status = StatusFile::from_str(sample_status()).unwrap();
        let new_tasks = vec![
            Task {
                id: "T1".to_string(),
                title: "Setup".to_string(),
                state: TaskState::Done,
                owner: "local_llm".to_string(),
                evidence: "ok".to_string(),
                notes: String::new(),
            },
            Task {
                id: "T2".to_string(),
                title: "Build".to_string(),
                state: TaskState::Done,
                owner: "claude".to_string(),
                evidence: "done".to_string(),
                notes: String::new(),
            },
        ];

        status.replace_task_table(&new_tasks);

        let tasks = status.tasks().unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[1].state, TaskState::Done);
        assert_eq!(tasks[1].owner, "claude");
    }

    #[test]
    fn replace_task_table_no_op_when_table_absent() {
        use crate::core::task::{Task, TaskState};

        let raw = "---\nrun_id: r\nprofile: local_only\ncycle: 0\nphase: plan\nlast_update: '2025-01-01T00:00:00Z'\nbudget:\n  paid_calls_used: 0\n  paid_calls_limit: 0\nlocks:\n  writer: null\n  active_task: null\n---\n\nNo task table here.\n";
        let mut status = StatusFile::from_str(raw).unwrap();
        let original_content = status.content.clone();

        status.replace_task_table(&[Task {
            id: "T1".to_string(),
            title: "Task".to_string(),
            state: TaskState::Todo,
            owner: String::new(),
            evidence: String::new(),
            notes: String::new(),
        }]);

        // Content should be unchanged because there was no table to replace.
        assert_eq!(status.content, original_content);
    }

    #[test]
    fn update_task_modifies_matching_row() {
        use crate::core::task::{Task, TaskState};

        let mut status = StatusFile::from_str(sample_status()).unwrap();
        let updated = Task {
            id: "T2".to_string(),
            title: "Build".to_string(),
            state: TaskState::Done,
            owner: "local_llm".to_string(),
            evidence: "verified".to_string(),
            notes: String::new(),
        };

        status.update_task(&updated).unwrap();

        let tasks = status.tasks().unwrap();
        let t2 = tasks.iter().find(|t| t.id == "T2").expect("T2 should exist");
        assert_eq!(t2.state, TaskState::Done);
        assert_eq!(t2.evidence, "verified");
    }

    #[test]
    fn update_task_leaves_unrelated_rows_unchanged() {
        use crate::core::task::{Task, TaskState};

        let mut status = StatusFile::from_str(sample_status()).unwrap();
        let updated = Task {
            id: "T2".to_string(),
            title: "Build".to_string(),
            state: TaskState::Done,
            owner: String::new(),
            evidence: String::new(),
            notes: String::new(),
        };

        status.update_task(&updated).unwrap();

        let tasks = status.tasks().unwrap();
        let t1 = tasks.iter().find(|t| t.id == "T1").expect("T1 should exist");
        assert_eq!(t1.state, TaskState::Done); // unchanged from sample
    }
}
