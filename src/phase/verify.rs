use std::path::Path;

use crate::agent::verifier;
use crate::core::cycle::CycleDecision;
use crate::core::status::StatusFile;
use crate::core::status_log;

/// Phase 6: Verify
/// Run verification commands from goal.md and report results.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
) -> anyhow::Result<CycleDecision> {
    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;

    // Extract verification commands from goal.md
    let commands = extract_verify_commands(&goal);

    if commands.is_empty() {
        status_log::append(
            &orch_dir.join("status_log.md"),
            "verify",
            "verifier",
            "orch",
            "No verification commands configured",
        )
        .await?;
        return Ok(CycleDecision::NextPhase);
    }

    // Run verification commands
    let result = verifier::run(&commands).await?;
    let formatted = verifier::format_result(&result);

    status_log::append(
        &orch_dir.join("status_log.md"),
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

/// Extract verification commands from goal.md.
/// Commands are in code blocks after "## Verification Commands".
fn extract_verify_commands(goal: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_verify_section = false;
    let mut in_code_block = false;

    for line in goal.lines() {
        if line.starts_with("## Verification") {
            in_verify_section = true;
            continue;
        }
        if in_verify_section && line.starts_with("## ") {
            break; // Next section
        }
        if in_verify_section {
            if line.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    commands.push(trimmed.to_string());
                }
            }
        }
    }

    commands
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
    use super::*;

    #[test]
    fn extract_commands_from_goal() {
        let goal = r#"# Goal

## Background

Some background.

## Verification Commands

```
cargo test
cargo clippy
```

## Quality Priority

speed
"#;
        let cmds = extract_verify_commands(goal);
        assert_eq!(cmds, vec!["cargo test", "cargo clippy"]);
    }

    #[test]
    fn extract_no_commands() {
        let goal = "# Goal\n\n## Background\n\nNo verify section.\n";
        let cmds = extract_verify_commands(goal);
        assert!(cmds.is_empty());
    }
}
