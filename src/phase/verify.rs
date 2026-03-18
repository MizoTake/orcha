use std::path::Path;

use crate::agent::verifier;
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, StopReason};
use crate::core::status::{StatusFile, VerifyStatus};
use crate::core::status_log;
use crate::machine_config::MachineConfig;

/// Phase 6: Verify
/// Run verification commands from orcha.yml and report results.
pub async fn execute(orch_dir: &Path, status: &mut StatusFile) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let commands = verification_commands_from_config(orch_dir)?;

    if commands.is_empty() {
        status.frontmatter.verify_status = Some(VerifyStatus::Skipped);
        status_log::append(
            &log_path,
            "verify",
            "verifier",
            "orch",
            "No verification commands configured in orcha.yml",
        )
        .await?;
        let verify_note = format!(
            "## Verification (Cycle {})\n\nVerification commands are not configured.\n\nOverall: SKIPPED",
            status.frontmatter.cycle
        );
        update_latest_notes(&mut status.content, &verify_note);
        return Ok(CycleDecision::Blocked(StopReason::VerificationNotConfigured));
    }

    // Run verification commands
    let result = verifier::run(&commands).await?;
    let formatted = verifier::format_result(&result);

    status_log::append(
        &log_path,
        "verify",
        "verifier",
        "orch",
        &format!("Verification: {}", result.summary),
    )
    .await?;

    // Update frontmatter with canonical verify status
    status.frontmatter.verify_status = Some(if result.passed {
        VerifyStatus::Pass
    } else {
        VerifyStatus::Fail
    });

    // Update latest notes with verification result
    let verify_note = format!(
        "## Verification (Cycle {})\n\n{}\n\nOverall: {}",
        status.frontmatter.cycle,
        formatted.trim(),
        if result.passed { "PASS" } else { "FAIL" }
    );
    update_latest_notes(&mut status.content, &verify_note);

    Ok(CycleDecision::NextPhase)
}

fn verification_commands_from_config(orch_dir: &Path) -> anyhow::Result<Vec<String>> {
    let cfg = MachineConfig::load(orch_dir)?;
    Ok(cfg.execution.verification.commands)
}

fn update_latest_notes(content: &mut String, note: &str) {
    if let Some(pos) = content.find("## Latest Notes") {
        let after_start = (pos + "## Latest Notes".len()).min(content.len());
        let after = &content[after_start..];
        let section_end = after
            .find("\n## ")
            .map(|p| after_start + p)
            .unwrap_or(content.len());
        *content = format!(
            "{}\n## Latest Notes\n\n{}\n{}",
            content[..pos].trim_end(),
            note,
            &content[section_end..]
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use crate::core::status::{ReviewStatus, StatusFrontmatter, Budget, Locks};
    use crate::machine_config::MachineConfig;

    use super::*;

    fn write_machine_config(dir: &Path, commands: &[&str]) {
        let mut cfg = MachineConfig::default();
        cfg.execution.verification.commands = commands.iter().map(|s| s.to_string()).collect();
        let yml = serde_yaml::to_string(&cfg).unwrap();
        std::fs::write(dir.join("orcha.yml"), yml).unwrap();
    }

    #[test]
    fn reads_commands_from_machine_config() {
        let temp = TempDir::new().unwrap();
        write_machine_config(temp.path(), &["cargo test", "cargo clippy"]);
        let cmds = verification_commands_from_config(temp.path()).unwrap();
        assert_eq!(cmds, vec!["cargo test", "cargo clippy"]);
    }

    #[test]
    fn reads_empty_commands_from_machine_config() {
        let temp = TempDir::new().unwrap();
        write_machine_config(temp.path(), &[]);
        let cmds = verification_commands_from_config(temp.path()).unwrap();
        assert!(cmds.is_empty());
    }

    #[tokio::test]
    async fn execute_blocks_when_verification_commands_are_missing() {
        let temp = TempDir::new().unwrap();
        write_machine_config(temp.path(), &[]);
        let workspace = temp.path().join("agentworkspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("status_log.md"), "# Status Log\n").unwrap();

        let mut status = StatusFile {
            frontmatter: StatusFrontmatter {
                run_id: "run-1".into(),
                profile: crate::core::profile::ProfileName::CheapCheckpoints,
                cycle: 0,
                phase: crate::core::cycle::Phase::Verify,
                last_update: chrono::Utc::now().to_rfc3339(),
                budget: Budget {
                    paid_calls_used: 0,
                    paid_calls_limit: 1,
                },
                locks: Locks {
                    writer: None,
                    active_task: None,
                },
                review_status: ReviewStatus::Clean,
                verify_status: None,
                consecutive_verify_failures: 0,
                disabled_agents: vec![],
            },
            content: "## Latest Notes\n\nInitialized.\n".into(),
        };

        let decision = execute(temp.path(), &mut status).await.unwrap();
        assert_eq!(decision, CycleDecision::Blocked(StopReason::VerificationNotConfigured));
        assert_eq!(status.frontmatter.verify_status, Some(VerifyStatus::Skipped));
    }
}
