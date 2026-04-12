use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoChangeSnapshot {
    entries: Vec<RepoChangeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RepoChangeEntry {
    status: String,
    path: String,
    content_hash: u64,
}

impl RepoChangeSnapshot {
    pub fn changed_paths(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.path.clone()).collect()
    }
}

pub async fn capture_repo_change_snapshot(orch_dir: &Path) -> RepoChangeSnapshot {
    let Some(workspace_root) = workspace_root(orch_dir) else {
        return RepoChangeSnapshot { entries: Vec::new() };
    };
    let orch_prefix = orch_dir_prefix(orch_dir, &workspace_root);
    let output = Command::new("git")
        .arg("-C")
        .arg(&workspace_root)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .await;
    let Ok(output) = output else {
        return RepoChangeSnapshot { entries: Vec::new() };
    };
    if !output.status.success() {
        return RepoChangeSnapshot { entries: Vec::new() };
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in raw.lines() {
        if let Some((status, path)) = parse_status_line(line) {
            if is_within_ignored_prefix(&path, orch_prefix.as_deref()) {
                continue;
            }
            let content_hash = hash_path_contents(&workspace_root, &path).await;
            entries.push(RepoChangeEntry { status, path, content_hash });
        }
    }
    entries.sort();
    RepoChangeSnapshot { entries }
}

fn parse_status_line(line: &str) -> Option<(String, String)> {
    if line.len() < 4 {
        return None;
    }
    let status = line[..2].to_string();
    let raw_path = line[3..].trim();
    if raw_path.is_empty() {
        return None;
    }
    let path = raw_path.rsplit(" -> ").next().unwrap_or(raw_path).trim().replace('\\', "/");
    if path.is_empty() {
        return None;
    }
    Some((status, path))
}

fn absolute_orch_dir(orch_dir: &Path) -> Option<PathBuf> {
    if orch_dir.is_absolute() {
        Some(orch_dir.to_path_buf())
    } else {
        Some(std::env::current_dir().ok()?.join(orch_dir))
    }
}

fn workspace_root(orch_dir: &Path) -> Option<PathBuf> {
    absolute_orch_dir(orch_dir)?.parent().map(Path::to_path_buf)
}

fn orch_dir_prefix(orch_dir: &Path, workspace_root: &Path) -> Option<String> {
    let absolute_orch_dir = absolute_orch_dir(orch_dir)?;
    let relative = absolute_orch_dir.strip_prefix(workspace_root).ok()?.to_string_lossy().replace('\\', "/");
    let trimmed = relative.trim_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn is_within_ignored_prefix(path: &str, prefix: Option<&str>) -> bool {
    let Some(prefix) = prefix else {
        return false;
    };
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

async fn hash_path_contents(workspace_root: &Path, path: &str) -> u64 {
    let absolute_path = workspace_root.join(path);
    let Ok(metadata) = tokio::fs::metadata(&absolute_path).await else {
        return 0;
    };
    if !metadata.is_file() {
        return 0;
    }
    let Ok(bytes) = tokio::fs::read(&absolute_path).await else {
        return 0;
    };
    hash_bytes(&bytes)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{capture_repo_change_snapshot, parse_status_line};
    use std::path::Path;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    #[test]
    fn parse_status_line_reads_plain_and_renamed_paths() {
        let modified = parse_status_line(" M src/lib.rs").expect("modified line should parse");
        assert_eq!(modified.0, " M");
        assert_eq!(modified.1, "src/lib.rs");

        let renamed = parse_status_line("R  src/old.rs -> src/new.rs").expect("rename line should parse");
        assert_eq!(renamed.0, "R ");
        assert_eq!(renamed.1, "src/new.rs");
    }

    #[tokio::test]
    async fn snapshot_changes_when_tracked_file_changes_again() {
        let temp = TempDir::new().expect("temp dir should be created");
        initialize_git_repo(temp.path(), &["src/work.txt"]);

        std::fs::write(temp.path().join("src").join("work.txt"), "dirty-one\n").expect("first dirty write should succeed");
        let before = capture_repo_change_snapshot(&temp.path().join(".orcha")).await;

        std::fs::write(temp.path().join("src").join("work.txt"), "dirty-two\n").expect("second dirty write should succeed");
        let after = capture_repo_change_snapshot(&temp.path().join(".orcha")).await;

        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn snapshot_changes_when_untracked_project_file_changes() {
        let temp = TempDir::new().expect("temp dir should be created");
        initialize_git_repo(temp.path(), &["src/work.txt"]);

        std::fs::write(temp.path().join("notes.md"), "first\n").expect("untracked file should be written");
        let before = capture_repo_change_snapshot(&temp.path().join(".orcha")).await;

        std::fs::write(temp.path().join("notes.md"), "second\n").expect("untracked file should update");
        let after = capture_repo_change_snapshot(&temp.path().join(".orcha")).await;

        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn snapshot_ignores_orcha_workspace_changes() {
        let temp = TempDir::new().expect("temp dir should be created");
        initialize_git_repo(temp.path(), &["src/work.txt"]);

        let orch_dir = temp.path().join(".orcha");
        std::fs::create_dir_all(orch_dir.join("tasks").join("open")).expect("orcha task dir should exist");
        std::fs::write(orch_dir.join("tasks").join("open").join("T1-test.md"), "initial\n").expect("orcha task file should be written");
        let before = capture_repo_change_snapshot(&orch_dir).await;

        std::fs::write(orch_dir.join("tasks").join("open").join("T1-test.md"), "updated\n").expect("orcha task file should update");
        let after = capture_repo_change_snapshot(&orch_dir).await;

        assert_eq!(before, after);
    }

    fn initialize_git_repo(workspace_root: &Path, tracked_files: &[&str]) {
        for file in tracked_files {
            let path = workspace_root.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("tracked file parent should exist");
            }
            std::fs::write(path, "baseline\n").expect("tracked file should be written");
        }

        run_git(workspace_root, &["init"]);
        run_git(workspace_root, &["config", "user.email", "test@example.com"]);
        run_git(workspace_root, &["config", "user.name", "orcha-test"]);

        let mut add_args = vec!["add"];
        add_args.extend(tracked_files.iter().copied());
        run_git(workspace_root, &add_args);
        run_git(workspace_root, &["commit", "-m", "baseline"]);
    }

    fn run_git(workspace_root: &Path, args: &[&str]) {
        let status = StdCommand::new("git").args(args).current_dir(workspace_root).status().expect("git should run");
        assert!(status.success(), "git {:?} should succeed", args);
    }
}
