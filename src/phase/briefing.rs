use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::CycleDecision;
use crate::core::handoff;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::TaskStore;
use crate::core::workspace_md;

/// Phase 1: Briefing
/// Read task files, status, inbox and prepare context summary for the cycle.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    // Load context files
    let role_path = workspace_md::resolve_role_file(orch_dir, "scribe")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("scribe.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;
    let inbox_path = workspace_md::resolve_handoff_file(orch_dir, "inbox")?;
    let inbox = handoff::read_handoff(&inbox_path).await?;

    let task_summary = task_store.render_summary_table().await?;

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "status.md".into(),
                content: format!(
                    "Cycle: {}\nPhase: {}\n\n{}",
                    status.frontmatter.cycle, status.frontmatter.phase, status.content
                ),
            },
            ContextFile {
                name: "tasks_summary".into(),
                content: task_summary,
            },
            ContextFile {
                name: role_name.clone(),
                content: role,
            },
        ],
        role: "scribe".to_string(),
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
    if response.is_paid {
        status.frontmatter.budget.paid_calls_used = status.frontmatter.budget.paid_calls_used.saturating_add(1);
    }
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

fn truncate_content(s: &str, max: usize) -> String {
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
        // Find the end of this section (next ## or end)
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
    use super::{truncate_content, update_latest_notes};

    #[test]
    fn truncate_content_returns_original_when_within_limit() {
        assert_eq!(truncate_content("hello", 10), "hello");
    }

    #[test]
    fn truncate_content_truncates_at_char_boundary() {
        // "テスト" is 9 bytes; slicing at byte 5 would be mid-char
        let s = "テスト合格";
        let result = truncate_content(s, 5);
        assert!(result.ends_with("... (truncated)"));
        let prefix = result.trim_end_matches("... (truncated)");
        assert!(s.starts_with(prefix));
    }

    #[test]
    fn update_latest_notes_replaces_section() {
        let mut content = "## Latest Notes\n\nOld note.\n".to_string();
        update_latest_notes(&mut content, "New note.");
        assert!(content.contains("New note."));
        assert!(!content.contains("Old note."));
    }

    #[test]
    fn update_latest_notes_handles_no_trailing_newline() {
        // Must not panic when "## Latest Notes" appears at end with no trailing \n
        let mut content = "## Latest Notes".to_string();
        update_latest_notes(&mut content, "Note.");
        assert!(content.contains("Note."));
    }
}
