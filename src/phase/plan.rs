use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{parse_task_table, Task, TaskState};
use crate::core::workspace_md;
use crate::machine_config::MachineConfig;

/// Phase 2: Plan
/// Planner agent creates or updates the task plan.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let existing_tasks = status.tasks()?;
    let has_remaining_work = existing_tasks.iter().any(|t| t.state != TaskState::Done);
    if !existing_tasks.is_empty() && has_remaining_work {
        status_log::append(
            &log_path,
            "plan",
            "planner",
            "orch",
            &format!(
                "Keeping existing task plan ({} tasks, remaining work detected)",
                existing_tasks.len()
            ),
        )
        .await?;
        return Ok(CycleDecision::NextPhase);
    }

    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;
    let role_path = workspace_md::resolve_role_file(orch_dir, "planner")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("planner.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "goal.md".into(),
                content: goal.clone(),
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
            "Review the goal and current status. Create or update the task plan.\n\
             Current cycle: {}\n\
             Return an updated task table in this exact format:\n\
             | ID | Title | State | Owner | Evidence | Notes |\n\
             |---|---|---|---|---|---|\n\
             | T1 | Task title | todo | agent_name | | description |\n\n\
             Also provide a brief plan rationale after the table.",
            status.frontmatter.cycle
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

    // Try to extract task table from the planner response.
    let mut applied_tasks = 0usize;
    if let Ok(new_tasks) = parse_task_table(&response.content) {
        if !new_tasks.is_empty() {
            applied_tasks = new_tasks.len();
            status.replace_task_table(&new_tasks);
        }
    }

    // Fallback: derive initial tasks from machine config acceptance criteria.
    if applied_tasks == 0 {
        let machine = MachineConfig::load(orch_dir)?;
        let fallback_tasks = tasks_from_acceptance_criteria(&machine.execution.acceptance_criteria);
        if !fallback_tasks.is_empty() {
            applied_tasks = fallback_tasks.len();
            status.replace_task_table(&fallback_tasks);
        }
    }

    status_log::append(
        &log_path,
        "plan",
        "planner",
        &response.model_used,
        &format!("Plan updated. Tasks initialized: {}", applied_tasks),
    )
    .await?;

    Ok(CycleDecision::NextPhase)
}

fn tasks_from_acceptance_criteria(criteria: &[String]) -> Vec<Task> {
    criteria
        .iter()
        .enumerate()
        .map(|(idx, c)| Task {
            id: format!("T{}", idx + 1),
            title: sanitize_table_cell(c),
            state: TaskState::Todo,
            owner: String::new(),
            evidence: String::new(),
            notes: "Derived from orcha.yml execution.acceptance_criteria".to_string(),
        })
        .collect()
}

fn sanitize_table_cell(s: &str) -> String {
    s.replace('|', "/").trim().to_string()
}

#[cfg(test)]
mod tests {
    use crate::core::task::TaskState;

    use super::tasks_from_acceptance_criteria;

    #[test]
    fn builds_tasks_from_acceptance_criteria() {
        let criteria = vec![
            "First criterion".to_string(),
            "Second criterion".to_string(),
        ];

        let tasks = tasks_from_acceptance_criteria(&criteria);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "T1");
        assert_eq!(tasks[0].state, TaskState::Todo);
        assert_eq!(tasks[1].id, "T2");
        assert_eq!(tasks[1].state, TaskState::Todo);
    }

    #[test]
    fn sanitizes_pipe_for_markdown_table_cell() {
        let criteria = vec!["API | auth".to_string()];

        let tasks = tasks_from_acceptance_criteria(&criteria);
        assert_eq!(tasks[0].title, "API / auth");
    }
}
