use std::path::Path;

use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::core::agent_workspace;
use crate::core::cycle::Phase;
use crate::core::status::StatusFile;

#[derive(Serialize)]
struct StructuredRunEvent<'a> {
    timestamp: String,
    run_id: &'a str,
    cycle: u32,
    phase: String,
    profile: String,
    event: &'a str,
    message: &'a str,
}

pub async fn append(
    orch_dir: &Path,
    status: &StatusFile,
    phase: Phase,
    event: &str,
    message: &str,
) -> anyhow::Result<()> {
    let path = agent_workspace::events_log_path(orch_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let record = StructuredRunEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        run_id: &status.frontmatter.run_id,
        cycle: status.frontmatter.cycle,
        phase: phase.to_string(),
        profile: status.frontmatter.profile.to_string(),
        event,
        message,
    };
    let serialized = serde_json::to_string(&record)?;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(serialized.as_bytes()).await?;
    file.write_all(b"\n").await?;
    Ok(())
}

