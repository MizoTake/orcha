use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::markdown::frontmatter::{self, Document};

// ---------------------------------------------------------------------------
// TaskState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Open,
    InProgress,
    Done,
    Blocked,
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskState::Open => write!(f, "open"),
            TaskState::InProgress => write!(f, "in-progress"),
            TaskState::Done => write!(f, "done"),
            TaskState::Blocked => write!(f, "blocked"),
        }
    }
}

impl TaskState {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "open" | "todo" | "issue" | "backlog" => Some(TaskState::Open),
            "in-progress" | "doing" | "wip" => Some(TaskState::InProgress),
            "done" => Some(TaskState::Done),
            "blocked" => Some(TaskState::Blocked),
            _ => None,
        }
    }

    pub fn folder_name(&self) -> &str {
        match self {
            TaskState::Open => "open",
            TaskState::InProgress => "in-progress",
            TaskState::Done => "done",
            TaskState::Blocked => "blocked",
        }
    }

    pub fn from_folder_name(name: &str) -> Option<Self> {
        Self::from_str(name)
    }

    pub const ALL: [TaskState; 4] = [
        TaskState::Open,
        TaskState::InProgress,
        TaskState::Done,
        TaskState::Blocked,
    ];

    pub fn legacy_folder_names(&self) -> &'static [&'static str] {
        match self {
            TaskState::Open => &["open", "todo", "issue", "backlog"],
            TaskState::InProgress => &["in-progress", "doing", "wip"],
            TaskState::Done => &["done"],
            TaskState::Blocked => &["blocked"],
        }
    }
}

// ---------------------------------------------------------------------------
// Task (lightweight summary, kept for table parsing from agent responses)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    pub owner: String,
    pub evidence: String,
    pub notes: String,
}

// ---------------------------------------------------------------------------
// TaskEntry (one .md file in the tasks folder tree)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFrontmatter {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub frontmatter: TaskFrontmatter,
    pub content: String,
    pub state: TaskState,
    pub file_name: String,
}

impl TaskEntry {
    /// Load a single task file.  `state` is inferred from the parent folder.
    pub async fn load(path: &Path, state: TaskState) -> anyhow::Result<Self> {
        let raw = tokio::fs::read_to_string(path).await?;
        let doc: Document<TaskFrontmatter> = frontmatter::parse(&raw)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.md")
            .to_string();
        Ok(TaskEntry {
            frontmatter: doc.frontmatter,
            content: doc.content,
            state,
            file_name,
        })
    }

    /// Serialize and write this entry to disk under `base_dir/<state>/`.
    pub async fn save(&self, base_dir: &Path) -> anyhow::Result<()> {
        let dir = base_dir.join(self.state.folder_name());
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(&self.file_name);
        let doc = Document {
            frontmatter: self.frontmatter.clone(),
            content: self.content.clone(),
        };
        let output = frontmatter::serialize(&doc)?;
        tokio::fs::write(&path, output).await?;
        Ok(())
    }

    /// Convert to a lightweight `Task` for summary/table rendering.
    pub fn to_task(&self) -> Task {
        Task {
            id: self.frontmatter.id.clone(),
            title: self.frontmatter.title.clone(),
            state: self.state,
            owner: self.frontmatter.owner.clone(),
            evidence: extract_section(&self.content, "Evidence"),
            notes: extract_section(&self.content, "Notes"),
        }
    }

    /// Generate a file name from id and title.
    pub fn generate_file_name(id: &str, title: &str) -> String {
        let slug: String = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let slug = slug.trim_matches('-').to_string();
        // Collapse consecutive dashes
        let mut prev = false;
        let slug: String = slug
            .chars()
            .filter(|&c| {
                if c == '-' {
                    if prev {
                        return false;
                    }
                    prev = true;
                } else {
                    prev = false;
                }
                true
            })
            .collect();
        let slug = if slug.len() > 40 { &slug[..40] } else { &slug };
        let slug = slug.trim_end_matches('-');
        format!("{}-{}.md", id, slug)
    }
}

fn extract_section(content: &str, heading: &str) -> String {
    let marker = format!("## {}", heading);
    if let Some(start) = content.find(&marker) {
        let after = &content[start + marker.len()..];
        let section_end = after.find("\n## ").unwrap_or(after.len());
        after[..section_end].trim().to_string()
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// TaskStore (folder-based task management)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TaskStore {
    pub base_dir: PathBuf,
}

impl TaskStore {
    pub fn new(orch_dir: &Path) -> Self {
        TaskStore {
            base_dir: orch_dir.join("tasks"),
        }
    }

    pub async fn ensure_dirs(&self) -> anyhow::Result<()> {
        for state in &TaskState::ALL {
            tokio::fs::create_dir_all(self.base_dir.join(state.folder_name())).await?;
        }
        Ok(())
    }

    /// List all tasks across all state folders, sorted by ID.
    pub async fn list_all(&self) -> anyhow::Result<Vec<TaskEntry>> {
        let mut entries = Vec::new();
        for state in &TaskState::ALL {
            let mut by_state = self.list_by_state(*state).await?;
            entries.append(&mut by_state);
        }
        entries.sort_by(|a, b| natural_sort_id(&a.frontmatter.id, &b.frontmatter.id));
        Ok(entries)
    }

    /// List tasks in a specific state folder.
    pub async fn list_by_state(&self, state: TaskState) -> anyhow::Result<Vec<TaskEntry>> {
        let mut entries = Vec::new();
        for dir_name in state.legacy_folder_names() {
            let dir = self.base_dir.join(dir_name);
            if !dir.exists() {
                continue;
            }
            let mut read_dir = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    match TaskEntry::load(&path, state).await {
                        Ok(task_entry) => entries.push(task_entry),
                        Err(e) => {
                            tracing::debug!("Skipping non-task file {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
        entries.sort_by(|a, b| natural_sort_id(&a.frontmatter.id, &b.frontmatter.id));
        Ok(entries)
    }

    /// Get the next open task (first by ID order).
    pub async fn next_open(&self) -> anyhow::Result<Option<TaskEntry>> {
        let issues = self.list_by_state(TaskState::Open).await?;
        Ok(issues.into_iter().next())
    }

    /// Move a task file from one state folder to another.
    /// Searches legacy folder names (e.g. `wip`, `issue`) so existing task
    /// directories created before the rename are handled transparently.
    pub async fn move_task(
        &self,
        file_name: &str,
        from: TaskState,
        to: TaskState,
    ) -> anyhow::Result<()> {
        let src = from
            .legacy_folder_names()
            .iter()
            .map(|dir| self.base_dir.join(dir).join(file_name))
            .find(|p| p.exists())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Task file '{}' not found in any '{}' folder",
                    file_name,
                    from
                )
            })?;
        let dst_dir = self.base_dir.join(to.folder_name());
        tokio::fs::create_dir_all(&dst_dir).await?;
        let dst = dst_dir.join(file_name);
        tokio::fs::rename(&src, &dst).await?;
        Ok(())
    }

    /// Create a new task in the canonical folder for its current state.
    pub async fn create_task(&self, entry: &TaskEntry) -> anyhow::Result<()> {
        entry.save(&self.base_dir).await
    }

    /// Update an existing task file in its current state folder.
    pub async fn update_task(&self, entry: &TaskEntry) -> anyhow::Result<()> {
        entry.save(&self.base_dir).await
    }

    /// Determine the next sequential task ID (T1, T2, ...).
    pub async fn next_id(&self) -> anyhow::Result<String> {
        let all = self.list_all().await?;
        let max_num = all
            .iter()
            .filter_map(|e| {
                e.frontmatter
                    .id
                    .strip_prefix('T')
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("T{}", max_num + 1))
    }

    /// Render a summary table of all tasks (for agent context).
    pub async fn render_summary_table(&self) -> anyhow::Result<String> {
        let all = self.list_all().await?;
        let tasks: Vec<Task> = all.iter().map(|e| e.to_task()).collect();
        Ok(render_task_table(&tasks))
    }

    /// Check if there are any tasks at all.
    pub async fn is_empty(&self) -> anyhow::Result<bool> {
        let all = self.list_all().await?;
        Ok(all.is_empty())
    }
}

/// Natural sort for task IDs like T1, T2, T10.
fn natural_sort_id(a: &str, b: &str) -> std::cmp::Ordering {
    let num_a = a.strip_prefix('T').and_then(|n| n.parse::<u32>().ok());
    let num_b = b.strip_prefix('T').and_then(|n| n.parse::<u32>().ok());
    match (num_a, num_b) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

// ---------------------------------------------------------------------------
// Task table parsing/rendering (for agent response parsing)
// ---------------------------------------------------------------------------

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

        let cols: Vec<&str> = trimmed.split('|').map(|s| s.trim()).collect::<Vec<_>>();

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
    fn task_frontmatter_parses_without_id_or_title() {
        // Agent-generated task files may omit id/title; #[serde(default)] must handle this.
        let raw = "---\nowner: agent\ncreated: '2026-01-01T00:00:00Z'\n---\n\n## Description\nDo something\n";
        let doc: crate::markdown::frontmatter::Document<TaskFrontmatter> =
            crate::markdown::frontmatter::parse(raw).unwrap();
        assert_eq!(doc.frontmatter.id, "");
        assert_eq!(doc.frontmatter.title, "");
        assert_eq!(doc.frontmatter.owner, "agent");
    }

    #[test]
    fn parse_empty_table() {
        let table =
            "| ID | Title | State | Owner | Evidence | Notes |\n|---|---|---|---|---|---|\n";
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
        assert_eq!(tasks[1].state, TaskState::InProgress);
        assert_eq!(tasks[1].notes, "WIP");
    }

    #[test]
    fn render_roundtrip() {
        let tasks = vec![Task {
            id: "T1".into(),
            title: "Setup".into(),
            state: TaskState::Open,
            owner: "local_llm".into(),
            evidence: "".into(),
            notes: "".into(),
        }];
        let rendered = render_task_table(&tasks);
        let parsed = parse_task_table(&rendered).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "T1");
        assert_eq!(parsed[0].state, TaskState::Open);
    }

    #[test]
    fn task_state_folder_names() {
        assert_eq!(TaskState::Open.folder_name(), "open");
        assert_eq!(TaskState::InProgress.folder_name(), "in-progress");
        assert_eq!(TaskState::Done.folder_name(), "done");
        assert_eq!(TaskState::Blocked.folder_name(), "blocked");
    }

    #[test]
    fn generate_file_name_basic() {
        let name = TaskEntry::generate_file_name("T1", "Setup database schema");
        assert_eq!(name, "T1-setup-database-schema.md");
    }

    #[test]
    fn generate_file_name_special_chars() {
        let name = TaskEntry::generate_file_name("T2", "Fix auth/crypto issues!");
        assert_eq!(name, "T2-fix-auth-crypto-issues.md");
    }

    #[test]
    fn natural_sort_order() {
        use std::cmp::Ordering;
        assert_eq!(natural_sort_id("T1", "T2"), Ordering::Less);
        assert_eq!(natural_sort_id("T2", "T10"), Ordering::Less);
        assert_eq!(natural_sort_id("T10", "T2"), Ordering::Greater);
    }

    #[test]
    fn extract_section_basic() {
        let content = "## Description\nHello world\n\n## Evidence\nSome evidence\n\n## Notes\nA note\n";
        assert_eq!(extract_section(content, "Evidence"), "Some evidence");
        assert_eq!(extract_section(content, "Notes"), "A note");
        assert_eq!(extract_section(content, "Missing"), "");
    }

    #[tokio::test]
    async fn task_store_create_and_list() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = TaskStore::new(temp.path());
        store.ensure_dirs().await.unwrap();

        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: "T1".into(),
                title: "Test task".into(),
                owner: String::new(),
                created: "2026-01-01T00:00:00Z".into(),
            },
            content: "## Description\nDo something\n".into(),
            state: TaskState::Open,
            file_name: "T1-test-task.md".into(),
        };
        store.create_task(&entry).await.unwrap();

        let all = store.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].frontmatter.id, "T1");
        assert_eq!(all[0].state, TaskState::Open);
    }

    #[tokio::test]
    async fn task_store_move_task() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = TaskStore::new(temp.path());
        store.ensure_dirs().await.unwrap();

        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: "T1".into(),
                title: "Test".into(),
                owner: String::new(),
                created: String::new(),
            },
            content: String::new(),
            state: TaskState::Open,
            file_name: "T1-test.md".into(),
        };
        store.create_task(&entry).await.unwrap();

        store
            .move_task("T1-test.md", TaskState::Open, TaskState::InProgress)
            .await
            .unwrap();

        let open_tasks = store.list_by_state(TaskState::Open).await.unwrap();
        let in_progress = store.list_by_state(TaskState::InProgress).await.unwrap();
        assert!(open_tasks.is_empty());
        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0].frontmatter.id, "T1");
    }

    #[tokio::test]
    async fn task_store_next_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = TaskStore::new(temp.path());
        store.ensure_dirs().await.unwrap();

        assert_eq!(store.next_id().await.unwrap(), "T1");

        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: "T3".into(),
                title: "Third".into(),
                owner: String::new(),
                created: String::new(),
            },
            content: String::new(),
            state: TaskState::Open,
            file_name: "T3-third.md".into(),
        };
        store.create_task(&entry).await.unwrap();

        assert_eq!(store.next_id().await.unwrap(), "T4");
    }

    #[test]
    fn legacy_state_names_still_parse() {
        assert_eq!(TaskState::from_str("issue"), Some(TaskState::Open));
        assert_eq!(TaskState::from_str("todo"), Some(TaskState::Open));
        assert_eq!(TaskState::from_str("backlog"), Some(TaskState::Open));
        assert_eq!(TaskState::from_str("wip"), Some(TaskState::InProgress));
        assert_eq!(TaskState::from_str("doing"), Some(TaskState::InProgress));
        assert_eq!(TaskState::from_folder_name("issue"), Some(TaskState::Open));
        assert_eq!(TaskState::from_folder_name("wip"), Some(TaskState::InProgress));
    }
}
