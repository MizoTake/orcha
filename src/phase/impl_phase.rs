use std::path::Path;
use std::sync::LazyLock;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{TaskState, TaskStore};
use crate::core::worktree;
use crate::core::workspace_md;
use regex::Regex;

static TASK_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z][0-9]+\b").expect("task id regex should compile"));

/// Phase 3: Implementation
/// Implementer agent executes the next `todo` task.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Find the next open task, preferring the planner's recommended task when available.
    let preferred_task_id = preferred_task_id_from_status(&status.content);
    let open_tasks = task_store.list_by_state(TaskState::Open).await?;
    let next_task = select_next_task(open_tasks, preferred_task_id.as_deref());
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
                "No open tasks available (some may be in-progress or blocked)",
            )
            .await?;
            return Ok(CycleDecision::NextPhase);
        }
    };

    // Move task from open → in-progress
    let file_name = task_entry.file_name.clone();
    let task_id = task_entry.frontmatter.id.clone();
    let task_title = task_entry.frontmatter.title.clone();
    task_entry.frontmatter.owner = "local_llm".to_string();
    let diff_before = worktree::capture_repo_change_snapshot(orch_dir).await;
    task_store
        .move_task(&file_name, TaskState::Open, TaskState::InProgress)
        .await?;
    task_entry.state = TaskState::InProgress;
    task_store.update_task(&task_entry).await?;
    status.frontmatter.locks.active_task = Some(task_id.clone());

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

    let diff_after = worktree::capture_repo_change_snapshot(orch_dir).await;
    let repo_changed = diff_after != diff_before;
    let reported_file_changes = response_reports_file_changes(&response.content);

    // Append evidence to the task content before deciding final state.
    let evidence_section = format!(
        "impl response by {}\n\n{}",
        response.model_used,
        response.content.trim()
    );
    update_section(&mut task_entry.content, "Evidence", &evidence_section);
    // Repository changes are the ground-truth evidence of completion.
    // A matching changed-files report in the response is a useful signal but
    // is not required when git already confirms that files were modified.
    if repo_changed {
        // Commit the implementation changes before updating task state
        git_commit(orch_dir, &format!("orcha: {} {}", task_id, task_title)).await;

        task_store
            .move_task(&file_name, TaskState::InProgress, TaskState::Done)
            .await?;
        task_entry.state = TaskState::Done;
        task_entry.frontmatter.owner = response.model_used.clone();
        task_store.update_task(&task_entry).await?;
        status.frontmatter.locks.active_task = None;

        let note = if reported_file_changes {
            format!("Completed task {}: {}", task_id, task_title)
        } else {
            format!(
                "Completed task {} (git evidence; response omitted changed-files report): {}",
                task_id, task_title
            )
        };
        status_log::append(&log_path, "impl", "implementer", &response.model_used, &note)
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
        .move_task(&file_name, TaskState::InProgress, TaskState::Blocked)
        .await?;
    task_entry.state = TaskState::Blocked;
    task_entry.frontmatter.owner = response.model_used.clone();
    update_section(
        &mut task_entry.content,
        "Notes",
        "Implementation did not produce a verifiable project change outside the orchestration workspace. Human follow-up required.",
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

/// Stage project changes and commit with the given message.
/// Failures are logged to stderr but do not abort the phase.
async fn git_commit(orch_dir: &Path, message: &str) {
    let stage_args = stage_project_changes_args(orch_dir);
    let stage = tokio::process::Command::new("git")
        .args(&stage_args)
        .output()
        .await;
    if let Err(e) = stage {
        eprintln!("  ⚠ git add failed: {}", e);
        return;
    }
    let commit = tokio::process::Command::new("git")
        .args(["commit", "-m", message])
        .output()
        .await;
    if let Err(e) = commit {
        eprintln!("  ⚠ git commit failed: {}", e);
    }
}

fn stage_project_changes_args(orch_dir: &Path) -> Vec<String> {
    let mut args = vec!["add".to_string(), "-A".to_string()];
    if let Some(prefix) = orch_dir_prefix(orch_dir) {
        args.push("--".to_string());
        args.push(".".to_string());
        args.push(format!(":(exclude){prefix}"));
    }
    args
}

fn orch_dir_prefix(orch_dir: &Path) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let absolute_orch_dir = if orch_dir.is_absolute() { orch_dir.to_path_buf() } else { cwd.join(orch_dir) };
    let relative = absolute_orch_dir.strip_prefix(&cwd).ok()?.to_string_lossy().replace('\\', "/");
    let trimmed = relative.trim_matches('/').to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn preferred_task_id_from_status(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        if !line.to_ascii_lowercase().contains("recommended first task") {
            return None;
        }
        TASK_ID_PATTERN.find(line).map(|m| m.as_str().to_string())
    })
}

fn select_next_task(open_tasks: Vec<crate::core::task::TaskEntry>, preferred_task_id: Option<&str>) -> Option<crate::core::task::TaskEntry> {
    if let Some(preferred_task_id) = preferred_task_id {
        if let Some(index) = open_tasks.iter().position(|task| task.frontmatter.id.eq_ignore_ascii_case(preferred_task_id)) {
            return open_tasks.into_iter().nth(index);
        }
    }
    open_tasks.into_iter().next()
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
    use crate::core::task::{TaskEntry, TaskFrontmatter, TaskState};
    use super::{implementation_completion_failure_reason, orch_dir_prefix, preferred_task_id_from_status, response_reports_file_changes, select_next_task, stage_project_changes_args};

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

    #[test]
    fn preferred_task_id_is_read_from_plan_note() {
        let status = "## Latest Notes\n\n## Plan (Cycle 3)\n\nRecommended first task: T3\n";
        assert_eq!(preferred_task_id_from_status(status), Some("T3".to_string()));
    }

    #[test]
    fn select_next_task_prefers_planner_recommendation() {
        let open_tasks = vec![
            TaskEntry {
                frontmatter: TaskFrontmatter { id: "T1".into(), title: "First".into(), owner: String::new(), created: String::new() },
                content: String::new(),
                state: TaskState::Open,
                file_name: "T1-first.md".into(),
            },
            TaskEntry {
                frontmatter: TaskFrontmatter { id: "T3".into(), title: "Third".into(), owner: String::new(), created: String::new() },
                content: String::new(),
                state: TaskState::Open,
                file_name: "T3-third.md".into(),
            },
        ];

        let selected = select_next_task(open_tasks, Some("T3")).expect("preferred task should be selected");
        assert_eq!(selected.frontmatter.id, "T3");
    }

    #[test]
    fn stage_project_changes_excludes_orcha_directory_when_possible() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let orch_dir = cwd.join(".orcha");
        let args = stage_project_changes_args(&orch_dir);

        assert!(args.iter().any(|arg| arg == ":(exclude).orcha"));
    }

    #[test]
    fn orch_dir_prefix_uses_relative_workspace_path() {
        let cwd = std::env::current_dir().expect("cwd should resolve");
        let orch_dir = cwd.join(".orcha");
        assert_eq!(orch_dir_prefix(&orch_dir), Some(".orcha".to_string()));
    }
}
