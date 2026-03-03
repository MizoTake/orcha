use std::path::Path;

use chrono::Utc;
use tokio::io::AsyncWriteExt;

/// Read the contents of a handoff markdown file.
pub async fn read_handoff(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        Ok(tokio::fs::read_to_string(path).await?)
    } else {
        Ok(String::new())
    }
}

/// Append a message to a handoff markdown file.
pub async fn append_handoff(path: &Path, from: &str, message: &str) -> anyhow::Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let entry = format!("\n---\n\n**{}** ({})\n\n{}\n", from, timestamp, message);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

/// Clear a handoff file (after processing).
pub async fn clear_handoff(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        let heading = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(to_heading)
            .unwrap_or_else(|| "Inbox".to_string());
        let content = format!("# {}\n\nNo pending messages.\n", heading);
        tokio::fs::write(path, content).await?;
    }
    Ok(())
}

fn to_heading(stem: &str) -> String {
    stem
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = String::new();
                    word.extend(first.to_uppercase());
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    // ── to_heading ───────────────────────────────────────────────────────────

    #[test]
    fn to_heading_single_word() {
        assert_eq!(super::to_heading("inbox"), "Inbox");
    }

    #[test]
    fn to_heading_underscore_separated() {
        assert_eq!(super::to_heading("some_inbox"), "Some Inbox");
    }

    #[test]
    fn to_heading_hyphen_separated() {
        assert_eq!(super::to_heading("agent-outbox"), "Agent Outbox");
    }

    #[test]
    fn to_heading_mixed_separators() {
        assert_eq!(super::to_heading("my_agent-inbox"), "My Agent Inbox");
    }

    #[test]
    fn to_heading_consecutive_separators_are_collapsed() {
        assert_eq!(super::to_heading("a__b--c"), "A B C");
    }

    // ── read_handoff / append_handoff / clear_handoff ─────────────────────

    #[tokio::test]
    async fn read_handoff_returns_empty_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("inbox.md");
        let content = read_handoff(&path).await.unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn append_handoff_creates_file_and_adds_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("inbox.md");

        append_handoff(&path, "agent-a", "hello from agent-a").await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("agent-a"));
        assert!(content.contains("hello from agent-a"));
    }

    #[tokio::test]
    async fn append_handoff_multiple_entries_accumulate() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("inbox.md");

        append_handoff(&path, "agent-a", "first message").await.unwrap();
        append_handoff(&path, "agent-b", "second message").await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("first message"));
        assert!(content.contains("second message"));
    }

    #[tokio::test]
    async fn clear_handoff_resets_file_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("inbox.md");

        append_handoff(&path, "agent", "some message").await.unwrap();
        clear_handoff(&path).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("No pending messages."));
        assert!(!content.contains("some message"));
    }

    #[tokio::test]
    async fn clear_handoff_uses_stem_as_heading() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("agent_inbox.md");

        tokio::fs::write(&path, "initial content").await.unwrap();
        clear_handoff(&path).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("# Agent Inbox"));
    }

    #[tokio::test]
    async fn clear_handoff_does_nothing_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.md");
        // Should not error
        clear_handoff(&path).await.unwrap();
        assert!(!path.exists());
    }
}
