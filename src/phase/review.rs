use std::path::Path;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::{ReviewStatus, StatusFile};
use crate::core::task::{TaskEntry, TaskFrontmatter, TaskState, TaskStore};
use crate::core::status_log;
use crate::core::workspace_md;

/// Phase 4: Review
/// Reviewer agent reviews the latest git commit.
/// If must-fix issues are found they are filed as issues for the next planning cycle.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let role_path = workspace_md::resolve_role_file(orch_dir, "reviewer")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("reviewer.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

    // Review only the latest commit (impl phase commits its changes)
    let diff = get_latest_commit_diff().await;
    let diff_lines = diff.as_ref().map(|d| d.lines().count()).unwrap_or(0);

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "status.md".into(),
                content: status.content.clone(),
            },
            ContextFile {
                name: role_name,
                content: role,
            },
            ContextFile {
                name: "latest_commit".into(),
                content: diff
                    .clone()
                    .unwrap_or_else(|| "No commit diff available.".into()),
            },
        ],
        role: "reviewer".to_string(),
        instruction: "Review the latest commit shown in latest_commit. Provide findings:\n\
             ```\n\
             Findings: High / Med / Low\n\
             Must-fix:\n\
             - item 1  (or '- (none)' if no blockers)\n\
             paid_review_required: yes/no\n\
             reason: explanation\n\
             ```"
            .to_string(),
    };

    let gate_ctx = GateContext {
        diff_content: diff,
        diff_lines,
        file_paths: latest_commit_file_paths().await,
        consecutive_verify_failures: status.frontmatter.consecutive_verify_failures,
    };

    let agent = router.select(Phase::Review, &gate_ctx);
    let response = agent.respond(&context).await?;
    if response.is_paid {
        status.frontmatter.budget.paid_calls_used =
            status.frontmatter.budget.paid_calls_used.saturating_add(1);
    }
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

    // If critical issues found: file them as an open task for the next cycle
    if has_must_fix {
        let title = format!("Review findings (cycle {})", status.frontmatter.cycle);
        let description = extract_must_fix_section(&response.content);
        let all_tasks = task_store.list_all().await.unwrap_or_default();
        let review_task_count = all_tasks
            .iter()
            .filter(|t| t.frontmatter.id.starts_with('R'))
            .count();
        let id = format!("R{}", review_task_count + 1);
        let file_name = TaskEntry::generate_file_name(&id, &title);
        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id,
                title,
                owner: String::new(),
                created: chrono::Utc::now().to_rfc3339(),
            },
            content: format!("## Description\n\n{}\n\n## Evidence\n\n\n\n## Notes\n\n\n", description),
            state: TaskState::Open,
            file_name,
        };
        if let Err(e) = task_store.create_task(&entry).await {
            eprintln!("  ⚠ Failed to create open task from review findings: {}", e);
        } else {
            status_log::append(
                &log_path,
                "review",
                "reviewer",
                &response.model_used,
                "Must-fix items filed as open task for next cycle",
            )
            .await?;
        }
    }

    // Review always sets Clean — issues are tracked in the issue store, not the fix phase
    status.frontmatter.review_status = ReviewStatus::Clean;

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

/// Extract must-fix items from the review response for issue filing.
fn extract_must_fix_section(response: &str) -> String {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in response.lines() {
        if line.trim_start().starts_with("Must-fix:") {
            in_section = true;
            lines.push(line.to_string());
            continue;
        }
        if in_section {
            // Stop at next key or end of block
            if line.trim().is_empty()
                || line.contains(':')
                    && !line.trim_start().starts_with('-')
                    && !line.trim_start().starts_with('*')
            {
                break;
            }
            lines.push(line.to_string());
        }
    }
    if lines.is_empty() {
        response.to_string()
    } else {
        lines.join("\n")
    }
}

/// Return the diff of the latest git commit (git show HEAD).
async fn get_latest_commit_diff() -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["show", "HEAD"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() { None } else { Some(diff) }
    } else {
        None
    }
}

/// Return the list of files changed in the latest commit.
async fn latest_commit_file_paths() -> Vec<String> {
    let output = tokio::process::Command::new("git")
        .args(["show", "--name-only", "--format=", "HEAD"])
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
