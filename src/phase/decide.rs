use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::agent::{AgentContext, ContextFile};
use crate::core::cycle::{CycleDecision, StopReason, MAX_CYCLES};
use crate::core::profile::ProfileName;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::TaskState;

/// Phase 7: Decide
/// Determine next action: next cycle, done, blocked, or escalate.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let tasks = status.tasks()?;

    // Check if all acceptance criteria are met (all tasks done + verify passed)
    let all_done = tasks.iter().all(|t| t.state == TaskState::Done);
    let verify_passed = status.content.contains("Overall: PASS");

    if all_done && verify_passed {
        status_log::append(
            &orch_dir.join("status_log.md"),
            "decide",
            "planner",
            "orch",
            "All tasks done and verification passed. Goal achieved!",
        )
        .await?;
        return Ok(CycleDecision::Done);
    }

    // Check stop conditions
    let next_cycle = status.frontmatter.cycle + 1;
    if next_cycle >= MAX_CYCLES {
        status_log::append(
            &orch_dir.join("status_log.md"),
            "decide",
            "planner",
            "orch",
            &format!("Maximum cycles ({}) reached", MAX_CYCLES),
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::MaxCyclesReached));
    }

    // Check for repeated failures
    let verify_failed = status.content.contains("Overall: FAIL");
    let has_blocked = tasks.iter().any(|t| t.state == TaskState::Blocked);

    if (verify_failed || has_blocked)
        && status.frontmatter.profile == ProfileName::LocalOnly
    {
        status_log::append(
            &orch_dir.join("status_log.md"),
            "decide",
            "planner",
            "orch",
            "Local-only profile stuck on failure",
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::LocalOnlyStuck));
    }

    // Ask planner agent for decision reasoning
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
            "Cycle {} has completed. Review the current state:\n\
             - Tasks done: {}/{}\n\
             - Verification: {}\n\
             - Blocked tasks: {}\n\n\
             Decide whether to:\n\
             1. Start next cycle (more work needed)\n\
             2. Mark as done (all criteria met)\n\
             3. Escalate (need human intervention)\n\n\
             Respond with exactly one of: NEXT_CYCLE, DONE, or ESCALATE: <reason>",
            status.frontmatter.cycle,
            tasks.iter().filter(|t| t.state == TaskState::Done).count(),
            tasks.len(),
            if verify_passed { "PASS" } else if verify_failed { "FAIL" } else { "NOT RUN" },
            tasks.iter().filter(|t| t.state == TaskState::Blocked).count()
        ),
    };

    let agent = router.default_agent();
    let response = agent.respond(&context).await?;

    let decision = parse_decision(&response.content);

    status_log::append(
        &orch_dir.join("status_log.md"),
        "decide",
        "planner",
        &response.model_used,
        &format!("Decision: {:?}", decision),
    )
    .await?;

    Ok(decision)
}

fn parse_decision(response: &str) -> CycleDecision {
    let upper = response.to_uppercase();
    if upper.contains("DONE") {
        CycleDecision::Done
    } else if upper.contains("ESCALATE") {
        let reason = response
            .split("ESCALATE")
            .nth(1)
            .map(|s| s.trim().trim_start_matches(':').trim())
            .unwrap_or("Agent requested escalation")
            .to_string();
        CycleDecision::Escalate(reason)
    } else {
        // Default to next cycle
        CycleDecision::NextCycle
    }
}
