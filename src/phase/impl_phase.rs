use std::path::Path;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{TaskState, TaskStore};
use crate::core::workspace_md;

/// Phase 3: Implementation
/// Implementer agent executes the next `todo` task.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Find the next todo task
    let next_task = task_store.next_issue().await?;
    let mut task_entry = match next_task {
        Some(entry) => entry,
        None => {
            // No todo tasks — check if all actionable work is finished
            let all = task_store.list_all().await?;
            if all
                .iter()
                .all(|t| matches!(t.state, TaskState::Done | TaskState::Blocked))
            {
                status_log::append(
                    &log_path,
                    "impl",
                    "implementer",
                    "orch",
                    "All tasks done, skipping impl phase",
                )
                .await?;
                return Ok(CycleDecision::NextPhase);
            }
            status_log::append(
                &log_path,
                "impl",
                "implementer",
                "orch",
                "No todo tasks available (some may be doing or blocked)",
            )
            .await?;
            return Ok(CycleDecision::NextPhase);
        }
    };

    // Move task from todo → doing
    let file_name = task_entry.file_name.clone();
    let task_id = task_entry.frontmatter.id.clone();
    let task_title = task_entry.frontmatter.title.clone();
    task_entry.frontmatter.owner = "local_llm".to_string();
    let diff_before = changed_files_snapshot().await;
    task_store
        .move_task(&file_name, TaskState::Todo, TaskState::Doing)
        .await?;
    task_entry.state = TaskState::Doing;
    task_store.update_task(&task_entry).await?;
    status.frontmatter.locks.active_task = Some(task_id.clone());

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
                name: format!("task_{}.md", task_id),
                content: task_entry.content.clone(),
            },
            ContextFile {
                name: role_name,
                content: role,
            },
        ],
        role: "implementer".to_string(),
        instruction: format!(
            "Implement the following task:\n\
             ID: {}\n\
             Title: {}\n\
             Details:\n{}\n\n\
             Provide the implementation and evidence of completion.",
            task_id, task_title, task_entry.content
        ),
    };

    let gate_ctx = GateContext::default();
    let agent = router.select(Phase::Impl, &gate_ctx);
    let response = agent.respond(&context).await?;
    if response.is_paid {
        status.frontmatter.budget.paid_calls_used =
            status.frontmatter.budget.paid_calls_used.saturating_add(1);
    }
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "impl",
        "implementer",
        &response.model_used,
        &response.content,
    )
    .await?;

    let diff_after = changed_files_snapshot().await;
    let repo_changed = diff_after != diff_before;
    let reported_file_changes = response_reports_file_changes(&response.content);

    // Append evidence to the task content before deciding final state.
    let evidence_section = format!(
        "impl response by {}\n\n{}",
        response.model_used,
        response.content.trim()
    );
    update_section(&mut task_entry.content, "Evidence", &evidence_section);
    if repo_changed && reported_file_changes {
        task_store
            .move_task(&file_name, TaskState::Doing, TaskState::Done)
            .await?;
        task_entry.state = TaskState::Done;
        task_entry.frontmatter.owner = response.model_used.clone();
        task_store.update_task(&task_entry).await?;
        status.frontmatter.locks.active_task = None;

        status_log::append(
            &log_path,
            "impl",
            "implementer",
            &response.model_used,
            &format!("Completed task {}: {}", task_id, task_title),
        )
        .await?;

        // Write the implementation response to outbox for external tools
        let outbox_path = workspace_md::resolve_handoff_file(orch_dir, "outbox")?;
        crate::core::handoff::append_handoff(
            &outbox_path,
            &format!("implementer({})", response.model_used),
            &response.content,
        )
        .await?;

        return Ok(CycleDecision::NextPhase);
    }

    task_store
        .move_task(&file_name, TaskState::Doing, TaskState::Blocked)
        .await?;
    task_entry.state = TaskState::Blocked;
    task_entry.frontmatter.owner = response.model_used.clone();
    update_section(
        &mut task_entry.content,
        "Notes",
        "Implementation did not provide both repository changes and an explicit changed-files report. Human follow-up required.",
    );
    task_store.update_task(&task_entry).await?;
    status.frontmatter.locks.active_task = None;

    status_log::append(
        &log_path,
        "impl",
        "implementer",
        &response.model_used,
        &format!(
            "Blocked task {}: implementation response did not satisfy completion evidence requirements",
            task_id
        ),
    )
    .await?;

    let failure_reason = implementation_completion_failure_reason(
        &response.model_used,
        repo_changed,
        reported_file_changes,
    );
    Ok(CycleDecision::Escalate(format!(
        "Task {} could not be completed automatically: {}",
        task_id, failure_reason
    )))
}

fn update_section(content: &mut String, heading: &str, new_text: &str) {
    let marker = format!("## {}", heading);
    if let Some(start) = content.find(&marker) {
        let after_marker = start + marker.len();
        let after = &content[after_marker..];
        let section_end = after
            .find("\n## ")
            .map(|p| after_marker + p)
            .unwrap_or(content.len());
        *content = format!(
            "{}{}\n\n{}\n\n{}",
            &content[..start],
            marker,
            new_text,
            &content[section_end..]
        );
    }
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

fn implementation_completion_failure_reason(
    model_used: &str,
    repo_changed: bool,
    reported_file_changes: bool,
) -> String {
    match (repo_changed, reported_file_changes) {
        (false, false) => format!(
            "no repository changes were detected and the response from {} did not include a changed-files report. If this agent is running in HTTP mode, switch the implementation-capable agent to CLI mode.",
            model_used
        ),
        (false, true) => format!(
            "the response from {} listed changed files, but no repository changes were detected. Check whether the agent can actually edit files in this environment.",
            model_used
        ),
        (true, false) => format!(
            "repository changes were detected, but the response from {} did not include the required changed-files report.",
            model_used
        ),
        (true, true) => "completion evidence check failed unexpectedly".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{implementation_completion_failure_reason, response_reports_file_changes};

    #[test]
    fn response_reports_file_changes_when_section_lists_paths() {
        let response = "Summary\nFiles modified:\n- src/lib.rs\n- README.md\nEvidence";
        assert!(response_reports_file_changes(response));
    }

    #[test]
    fn response_reports_file_changes_requires_explicit_section_and_path() {
        let response = "Summary\nUpdated implementation and tests passed.";
        assert!(!response_reports_file_changes(response));
    }

    #[test]
    fn failure_reason_mentions_cli_mode_when_no_changes_detected() {
        let reason = implementation_completion_failure_reason("gpt-4.1", false, false);
        assert!(reason.contains("CLI mode"));
        assert!(reason.contains("gpt-4.1"));
    }
}
