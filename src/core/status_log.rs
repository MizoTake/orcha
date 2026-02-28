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
    let line = format!("{} [{}] {}({}): {}\n", timestamp, phase, role, agent, message);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;

    file.write_all(line.as_bytes()).await?;
    Ok(())
}
