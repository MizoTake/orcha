use std::path::Path;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::TaskState;
use crate::core::workspace_md;

/// Phase 3: Implementation
/// Implementer agent executes the next `todo` task.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let mut tasks = status.tasks()?;

    // Find the next todo task
    let task_idx = tasks.iter().position(|t| t.state == TaskState::Todo);
    let task_idx = match task_idx {
        Some(idx) => idx,
        None => {
            // No todo tasks, check if all done
            if tasks.iter().all(|t| t.state == TaskState::Done) {
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
                "No todo tasks available (some may be blocked)",
            )
            .await?;
            return Ok(CycleDecision::NextPhase);
        }
    };

    // Mark task as doing
    tasks[task_idx].state = TaskState::Doing;
    tasks[task_idx].owner = "local_llm".to_string();
    status.replace_task_table(&tasks);
    status.frontmatter.locks.active_task = Some(tasks[task_idx].id.clone());

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
        instruction: format!(
            "Implement the following task:\n\
             ID: {}\n\
             Title: {}\n\
             Notes: {}\n\n\
             Provide the implementation and evidence of completion.",
            tasks[task_idx].id, tasks[task_idx].title, tasks[task_idx].notes
        ),
    };

    let gate_ctx = GateContext::default();
    let agent = router.select(Phase::Impl, &gate_ctx);
    let response = agent.respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "impl",
        "implementer",
        &response.model_used,
        &response.content,
    )
    .await?;

    // Mark task as done
    tasks[task_idx].state = TaskState::Done;
    tasks[task_idx].evidence = "impl completed".to_string();
    tasks[task_idx].owner = response.model_used.clone();
    status.replace_task_table(&tasks);
    status.frontmatter.locks.active_task = None;

    status_log::append(
        &log_path,
        "impl",
        "implementer",
        &response.model_used,
        &format!(
            "Completed task {}: {}",
            tasks[task_idx].id, tasks[task_idx].title
        ),
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

    Ok(CycleDecision::NextPhase)
}
