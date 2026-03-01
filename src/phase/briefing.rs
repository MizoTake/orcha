use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::handoff;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::workspace_md;

/// Phase 1: Briefing
/// Read goal, status, inbox and prepare context summary for the cycle.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Load context files
    let goal = tokio::fs::read_to_string(orch_dir.join("goal.md")).await?;
    let role_path = workspace_md::resolve_role_file(orch_dir, "scribe")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("scribe.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;
    let inbox_path = workspace_md::resolve_handoff_file(orch_dir, "inbox")?;
    let inbox = handoff::read_handoff(&inbox_path).await?;

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "goal.md".into(),
                content: goal,
            },
            ContextFile {
                name: "status.md".into(),
                content: format!(
                    "Cycle: {}\nPhase: {}\n\n{}",
                    status.frontmatter.cycle, status.frontmatter.phase, status.content
                ),
            },
            ContextFile {
                name: role_name.clone(),
                content: role,
            },
        ],
        instruction: format!(
            "Prepare a briefing for cycle {}. Summarize the current state, \
             what has been accomplished, what remains, and recommend focus areas. \
             {}",
            status.frontmatter.cycle,
            if inbox.contains("No pending messages") {
                "No inbox messages.".to_string()
            } else {
                format!("Inbox messages:\n{}", inbox)
            }
        ),
    };

    let agent = router.default_agent();
    let response = agent.respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "briefing",
        "scribe",
        &response.model_used,
        &response.content,
    )
    .await?;

    // Log the briefing
    status_log::append(
        &log_path,
        "briefing",
        "scribe",
        &response.model_used,
        "Briefing completed",
    )
    .await?;

    // Clear inbox after processing
    if !inbox.contains("No pending messages") {
        handoff::clear_handoff(&inbox_path).await?;
    }

    // Update status notes with briefing summary
    let briefing_note = format!(
        "## Briefing (Cycle {})\n\n{}",
        status.frontmatter.cycle,
        truncate_content(&response.content, 500)
    );
    update_latest_notes(&mut status.content, &briefing_note);

    Ok(CycleDecision::NextPhase)
}

fn truncate_content(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

fn update_latest_notes(content: &mut String, note: &str) {
    if let Some(pos) = content.find("## Latest Notes") {
        // Find the end of this section (next ## or end)
        let after = &content[pos + 16..];
        let section_end = after
            .find("\n## ")
            .map(|p| pos + 16 + p)
            .unwrap_or(content.len());
        *content = format!(
            "{}\n## Latest Notes\n\n{}\n{}",
            content[..pos].trim_end(),
            note,
            &content[section_end..]
        );
    }
}
