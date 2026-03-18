use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, StopReason};
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{TaskState, TaskStore};
use crate::core::workspace_md;

/// Phase 2: Plan
/// Planner agent reviews open tasks and produces an implementation strategy.
/// The strategy is written to status notes so the impl agent can reference it.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let all_tasks = task_store.list_all().await?;

    if all_tasks.is_empty() {
        status_log::append(
            &log_path,
            "plan",
            "planner",
            "orch",
            "No task files found in tasks/open. Please add markdown files to .orcha/tasks/open/",
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::NoTasksFound));
    }

    let open_tasks: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.state == TaskState::Open)
        .collect();

    if open_tasks.is_empty() {
        status_log::append(
            &log_path,
            "plan",
            "planner",
            "orch",
            &format!(
                "No open tasks remaining ({} tasks total, all done or blocked)",
                all_tasks.len()
            ),
        )
        .await?;
        return Ok(CycleDecision::NextPhase);
    }

    let role_path = workspace_md::resolve_role_file(orch_dir, "planner")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("planner.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

    // Build task list with full content for planner context
    let open_tasks_content: String = open_tasks
        .iter()
        .map(|t| {
            format!(
                "### {} — {}\n\n{}\n",
                t.frontmatter.id,
                t.frontmatter.title,
                t.content.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n\n");

    let done_count = all_tasks.iter().filter(|t| t.state == TaskState::Done).count();
    let blocked_count = all_tasks.iter().filter(|t| t.state == TaskState::Blocked).count();

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "open_tasks.md".into(),
                content: open_tasks_content,
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
        role: "planner".to_string(),
        instruction: format!(
            "Review the open tasks and produce an implementation strategy for this cycle.\n\
             Cycle: {}\n\
             Open: {} / Done: {} / Blocked: {}\n\n\
             Your response must include:\n\
             1. **Recommended order** — which task to tackle first and why\n\
             2. **Dependencies** — any ordering constraints between tasks\n\
             3. **Risks** — potential blockers or unknowns to watch for\n\
             4. **Approach notes** — brief implementation guidance per task\n\n\
             Keep the response concise. The implementer agent will read this plan.",
            status.frontmatter.cycle,
            open_tasks.len(),
            done_count,
            blocked_count,
        ),
    };

    let agent = router.default_agent();
    let response = agent.respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "plan",
        "planner",
        &response.model_used,
        &response.content,
    )
    .await?;

    status_log::append(
        &log_path,
        "plan",
        "planner",
        &response.model_used,
        &format!(
            "Strategy planned for {} open task(s)",
            open_tasks.len()
        ),
    )
    .await?;

    // Write plan summary to status notes so impl agent can reference it
    let plan_note = format!(
        "## Plan (Cycle {})\n\n{}",
        status.frontmatter.cycle,
        truncate_plan(&response.content, 800)
    );
    update_latest_notes(&mut status.content, &plan_note);

    Ok(CycleDecision::NextPhase)
}

fn truncate_plan(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let boundary = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= max)
        .last()
        .unwrap_or(0);
    format!("{}... (truncated)", &s[..boundary])
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
    use super::{truncate_plan, update_latest_notes};

    #[test]
    fn truncate_plan_within_limit_is_unchanged() {
        assert_eq!(truncate_plan("hello", 10), "hello");
    }

    #[test]
    fn truncate_plan_over_limit_appends_suffix() {
        let result = truncate_plan("hello world", 5);
        assert!(result.ends_with("... (truncated)"));
    }

    #[test]
    fn update_latest_notes_replaces_section() {
        let mut content = "## Latest Notes\n\nOld.\n".to_string();
        update_latest_notes(&mut content, "New plan.");
        assert!(content.contains("New plan."));
        assert!(!content.contains("Old."));
    }

    #[test]
    fn update_latest_notes_no_panic_at_end_of_string() {
        let mut content = "## Latest Notes".to_string();
        update_latest_notes(&mut content, "Note.");
        assert!(content.contains("Note."));
    }
}
