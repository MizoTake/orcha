use std::path::Path;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::workspace_md;

/// Phase 4: Review
/// Reviewer agent reviews the implementation changes.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;
    let role_path = workspace_md::resolve_role_file(orch_dir, "reviewer")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("reviewer.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

    // Try to get git diff for review context
    let diff = get_git_diff().await;
    let diff_lines = diff.as_ref().map(|d| d.lines().count()).unwrap_or(0);

    // Read outbox to see latest implementation output
    let outbox_path = workspace_md::resolve_handoff_file(orch_dir, "outbox")?;
    let outbox = crate::core::handoff::read_handoff(&outbox_path).await?;

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "goal.md".into(),
                content: goal,
            },
            ContextFile {
                name: "status.md".into(),
                content: status.content.clone(),
            },
            ContextFile {
                name: role_name,
                content: role,
            },
            ContextFile {
                name: "recent_changes".into(),
                content: diff
                    .clone()
                    .or_else(|| {
                        if outbox.contains("No pending messages") {
                            None
                        } else {
                            Some(outbox)
                        }
                    })
                    .unwrap_or_else(|| "No changes detected.".into()),
            },
        ],
        role: "reviewer".to_string(),
        instruction: "Review the recent implementation changes. Provide findings in the format:\n\
             ```\n\
             Findings: High / Med / Low\n\
             Must-fix:\n\
             - item 1\n\
             paid_review_required: yes/no\n\
             reason: explanation\n\
             ```"
        .to_string(),
    };

    let gate_ctx = GateContext {
        diff_content: diff,
        diff_lines,
        file_paths: Vec::new(),
        consecutive_verify_failures: 0,
    };

    let agent = router.select(Phase::Review, &gate_ctx);
    let response = agent.respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "review",
        "reviewer",
        &response.model_used,
        &response.content,
    )
    .await?;

    // Parse review findings
    let has_must_fix = response.content.contains("Must-fix:")
        && !response.content.contains("Must-fix:\n- (none)")
        && !response.content.contains("Must-fix:\nNone");

    let needs_paid = response.content.contains("paid_review_required: yes");

    status_log::append(
        &log_path,
        "review",
        "reviewer",
        &response.model_used,
        &format!(
            "Review completed. Must-fix: {}, Paid review needed: {}",
            has_must_fix, needs_paid
        ),
    )
    .await?;

    // Store review output in status notes
    let review_note = format!(
        "## Review (Cycle {})\n\n{}",
        status.frontmatter.cycle, &response.content
    );
    update_latest_notes(&mut status.content, &review_note);

    if needs_paid && !response.is_paid {
        return Ok(CycleDecision::Escalate(
            "Reviewer recommends paid review".into(),
        ));
    }

    Ok(CycleDecision::NextPhase)
}

async fn get_git_diff() -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["diff", "HEAD"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() {
            None
        } else {
            Some(diff)
        }
    } else {
        None
    }
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
