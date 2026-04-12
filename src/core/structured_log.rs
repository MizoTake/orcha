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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::append;
    use crate::core::agent_workspace;
    use crate::core::cycle::Phase;
    use crate::core::profile::ProfileName;
    use crate::core::status::{Budget, Locks, ReviewStatus, StatusFile, StatusFrontmatter};

    fn sample_status() -> StatusFile {
        StatusFile {
            frontmatter: StatusFrontmatter {
                run_id: "run-1".into(),
                profile: ProfileName::CheapCheckpoints,
                cycle: 2,
                phase: Phase::Review,
                last_update: "2026-01-01T00:00:00Z".into(),
                budget: Budget { paid_calls_used: 1, paid_calls_limit: 3 },
                locks: Locks { writer: None, active_task: None },
                review_status: ReviewStatus::Clean,
                verify_status: None,
                consecutive_verify_failures: 0,
                disabled_agents: vec![],
            },
            content: String::new(),
        }
    }

    #[tokio::test]
    async fn append_writes_json_line_with_expected_fields() {
        let temp = TempDir::new().expect("temp dir should be created");
        let status = sample_status();

        append(temp.path(), &status, Phase::Review, "phase_result", "Review completed").await.expect("append should succeed");

        let content = tokio::fs::read_to_string(agent_workspace::events_log_path(temp.path())).await.expect("events log should be readable");
        let value: serde_json::Value = serde_json::from_str(content.trim()).expect("line should be valid json");
        assert_eq!(value["run_id"], "run-1");
        assert_eq!(value["cycle"], 2);
        assert_eq!(value["phase"], "review");
        assert_eq!(value["profile"], "cheap_checkpoints");
        assert_eq!(value["event"], "phase_result");
        assert_eq!(value["message"], "Review completed");
    }
}
