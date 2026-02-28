use std::path::Path;

use crate::agent::verifier;
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::machine_config::MachineConfig;

/// Phase 6: Verify
/// Run verification commands from orcha.yml and report results.
pub async fn execute(orch_dir: &Path, status: &mut StatusFile) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let commands = verification_commands_from_config(orch_dir)?;

    if commands.is_empty() {
        status_log::append(
            &log_path,
            "verify",
            "verifier",
            "orch",
            "No verification commands configured in orcha.yml",
        )
        .await?;
        return Ok(CycleDecision::NextPhase);
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
        let after = &content[pos + 16..];
        let section_end = after
            .find("\n## ")
            .map(|p| pos + 16 + p)
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
}
