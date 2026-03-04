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
/// Implementer agent executes the next `issue` task.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Find the next issue task
    let next_task = task_store.next_issue().await?;
    let mut task_entry = match next_task {
        Some(entry) => entry,
        None => {
            // No issue tasks — check if all done
            let all = task_store.list_all().await?;
            if all.iter().all(|t| t.state == TaskState::Done) {
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
                "No issue tasks available (some may be wip)",
            )
            .await?;
            return Ok(CycleDecision::NextPhase);
        }
    };

    // Move task from issue → wip
    let file_name = task_entry.file_name.clone();
    let task_id = task_entry.frontmatter.id.clone();
    let task_title = task_entry.frontmatter.title.clone();
    task_entry.frontmatter.owner = "local_llm".to_string();
    task_store
        .move_task(&file_name, TaskState::Issue, TaskState::Wip)
        .await?;
    task_entry.state = TaskState::Wip;
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
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "impl",
        "implementer",
        &response.model_used,
        &response.content,
    )
    .await?;

    // Move task from wip → done, update content with evidence
    task_store
        .move_task(&file_name, TaskState::Wip, TaskState::Done)
        .await?;
    task_entry.state = TaskState::Done;
    task_entry.frontmatter.owner = response.model_used.clone();

    // Append evidence to the task content
    let evidence_section = format!("impl completed by {}", response.model_used);
    update_section(&mut task_entry.content, "Evidence", &evidence_section);
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

    Ok(CycleDecision::NextPhase)
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
