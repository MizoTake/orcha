use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::cycle::CycleDecision;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::parse_task_table;

/// Phase 2: Plan
/// Planner agent creates or updates the task plan.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;
    let role = tokio::fs::read_to_string(orch_dir.join("roles").join("planner.md")).await?;

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
                name: "planner.md".into(),
                content: role,
            },
        ],
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

    // Try to extract task table from the response
    if let Ok(new_tasks) = parse_task_table(&response.content) {
        if !new_tasks.is_empty() {
            status.replace_task_table(&new_tasks);
        }
    }

    status_log::append(
        &orch_dir.join("status_log.md"),
        "plan",
        "planner",
        &response.model_used,
        &format!(
            "Plan updated. Tasks: {}",
            status.tasks().map(|t| t.len()).unwrap_or(0)
        ),
    )
    .await?;

    Ok(CycleDecision::NextPhase)
}
