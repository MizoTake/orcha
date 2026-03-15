use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::status::{ReviewStatus, StatusFile};
use crate::core::status_log;
use crate::core::workspace_md;

/// Phase 5: Fix
/// If review found must-fix issues, send to implementer agent with fix instructions.
/// If no issues, skip to next phase.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Check if there are must-fix items in the latest notes
    let has_fixes_needed = status.content.contains("Must-fix:")
        && !status.content.contains("Must-fix:\n- (none)")
        && !status.content.contains("Must-fix:\nNone");

    if !has_fixes_needed {
        status_log::append(
            &log_path,
            "fix",
            "implementer",
            "orch",
            "No fixes needed, skipping",
        )
        .await?;
        return Ok(CycleDecision::NextPhase);
    }

    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;
    let role_path = workspace_md::resolve_role_file(orch_dir, "implementer")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("implementer.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

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
        ],
        role: "implementer".to_string(),
        instruction:
            "Review the must-fix items from the review phase and apply the necessary fixes.\n\
             The review findings are in the Latest Notes section of status.md.\n\
             End your response with `Resolved: yes` only when every must-fix item is addressed.\n\
             Otherwise end with `Resolved: no` and explain the remaining blocker."
                .to_string(),
    };

    let agent = router.default_agent();
    let diff_before = changed_files_snapshot().await;
    let response = agent.respond(&context).await?;
    if response.is_paid {
        status.frontmatter.budget.paid_calls_used =
            status.frontmatter.budget.paid_calls_used.saturating_add(1);
    }
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "fix",
        "implementer",
        &response.model_used,
        &response.content,
    )
    .await?;

    let resolved = response.content.contains("Resolved: yes");
    let diff_after = changed_files_snapshot().await;
    let repo_changed = diff_before != diff_after;
    let reported_file_changes = response_reports_file_changes(&response.content);

    if !resolved || !repo_changed || !reported_file_changes {
        status.frontmatter.review_status = ReviewStatus::IssuesFound;
        status_log::append(
            &log_path,
            "fix",
            "implementer",
            &response.model_used,
            "Fix phase could not verify all must-fix items as resolved",
        )
        .await?;
        return Ok(CycleDecision::Escalate(
            "Fix phase did not resolve all must-fix items".to_string(),
        ));
    }

    status.frontmatter.review_status = ReviewStatus::Clean;

    status_log::append(
        &log_path,
        "fix",
        "implementer",
        &response.model_used,
        "Fixes applied",
    )
    .await?;

    // Write fix output to outbox
    let outbox_path = workspace_md::resolve_handoff_file(orch_dir, "outbox")?;
    crate::core::handoff::append_handoff(
        &outbox_path,
        &format!("implementer({})", response.model_used),
        &response.content,
    )
    .await?;

    Ok(CycleDecision::NextPhase)
}

async fn changed_files_snapshot() -> Vec<String> {
    let output = tokio::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output()
        .await;
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn response_reports_file_changes(response: &str) -> bool {
    let lower = response.to_ascii_lowercase();
    if !(lower.contains("files modified")
        || lower.contains("files changed")
        || lower.contains("files created")
        || lower.contains("changed files"))
    {
        return false;
    }

    response.lines().any(|line| {
        let trimmed = line.trim();
        (trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("1."))
            && (trimmed.contains('/') || trimmed.contains('\\') || trimmed.ends_with(".rs") || trimmed.ends_with(".md"))
    })
}

#[cfg(test)]
mod tests {
    use super::response_reports_file_changes;

    #[test]
    fn fix_response_reports_file_changes_when_paths_are_listed() {
        let response = "Resolved: yes\nFiles changed:\n- src/lib.rs\n- src/main.rs";
        assert!(response_reports_file_changes(response));
    }

    #[test]
    fn fix_response_without_file_report_is_rejected() {
        let response = "Resolved: yes\nApplied the fix.";
        assert!(!response_reports_file_changes(response));
    }
}
