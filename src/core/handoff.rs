use std::path::Path;

use chrono::Utc;
use tokio::io::AsyncWriteExt;

/// Read the contents of a handoff file (inbox.md or outbox.md).
pub async fn read_handoff(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        Ok(tokio::fs::read_to_string(path).await?)
    } else {
        Ok(String::new())
    }
}

/// Append a message to a handoff file.
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
        tokio::fs::write(path, "# Inbox\n\nNo pending messages.\n").await?;
    }
    Ok(())
}
