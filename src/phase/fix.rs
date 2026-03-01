use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::status::StatusFile;
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
        instruction:
            "Review the must-fix items from the review phase and apply the necessary fixes.\n\
             The review findings are in the Latest Notes section of status.md.\n\
             Provide the fixes and evidence of completion."
                .to_string(),
    };

    let agent = router.default_agent();
    let response = agent.respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "fix",
        "implementer",
        &response.model_used,
        &response.content,
    )
    .await?;

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
