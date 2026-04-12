use std::path::Path;

use chrono::Utc;
use tokio::io::AsyncWriteExt;

/// Append a log entry to status_log.md.
/// Format: `time [phase] role(agent): message`
/// This file is append-only; no edits allowed.
pub async fn append(
    path: &Path,
    phase: &str,
    role: &str,
    agent: &str,
    message: &str,
) -> anyhow::Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!(
        "{} [{}] {}({}): {}\n",
        timestamp, phase, role, agent, message
    );

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(line.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::append;

    #[tokio::test]
    async fn append_creates_log_file_and_writes_expected_fields() {
        let temp = TempDir::new().expect("temp dir should be created");
        let path = temp.path().join("status_log.md");

        append(&path, "impl", "implementer", "local_llm", "finished work").await.expect("append should succeed");

        let content = tokio::fs::read_to_string(&path).await.expect("log should be readable");
        assert!(content.contains("[impl]"));
        assert!(content.contains("implementer(local_llm): finished work"));
        assert!(content.ends_with('\n'));
    }

    #[tokio::test]
    async fn append_is_append_only() {
        let temp = TempDir::new().expect("temp dir should be created");
        let path = temp.path().join("status_log.md");

        append(&path, "briefing", "scribe", "local_llm", "first").await.expect("first append should succeed");
        append(&path, "plan", "planner", "local_llm", "second").await.expect("second append should succeed");

        let content = tokio::fs::read_to_string(&path).await.expect("log should be readable");
        assert!(content.contains("[briefing] scribe(local_llm): first"));
        assert!(content.contains("[plan] planner(local_llm): second"));
        assert_eq!(content.lines().count(), 2);
    }
}
