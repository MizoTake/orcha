use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, StopReason};
use crate::core::profile::ProfileName;
use crate::core::status::{StatusFile, VerifyStatus};
use crate::core::status_log;
use crate::core::task::{TaskState, TaskStore};
use crate::machine_config::MachineConfig;

/// Phase 7: Decide
/// Determine next action: next cycle, done, blocked, or escalate.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    _router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let all_tasks = task_store.list_all().await?;
    let machine = MachineConfig::load(orch_dir)?;
    let max_cycles = machine.execution.max_cycles;
    let criteria_count = machine.execution.acceptance_criteria.len();

    let done_count = all_tasks.iter().filter(|t| t.state == TaskState::Done).count();
    let total = all_tasks.len();
    let all_done = total > 0 && done_count == total;
    let verify_passed = status.frontmatter.verify_status == Some(VerifyStatus::Pass);

    if completion_satisfied(all_done, verify_passed, done_count, criteria_count) {
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            &format!(
                "Goal achieved. Tasks done: {}/{}; acceptance criteria: {}",
                done_count, total, criteria_count
            ),
        )
        .await?;
        return Ok(CycleDecision::Done);
    }

    let verify_failed = status.frontmatter.verify_status == Some(VerifyStatus::Fail);

    // Check stop conditions
    let next_cycle = status.frontmatter.cycle + 1;
    if max_cycles > 0 && next_cycle >= max_cycles {
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            &format!("Maximum cycles ({}) reached", max_cycles),
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::MaxCyclesReached));
    }

    if verify_failed && status.frontmatter.profile == ProfileName::LocalOnly {
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            "Local-only profile stuck on failure",
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::LocalOnlyStuck));
    }

    status_log::append(
        &log_path,
        "decide",
        "planner",
        "orch",
        &format!(
            "Continuing to next cycle. Tasks done: {}/{}; verify_passed: {}; acceptance criteria: {}",
            done_count, total, verify_passed, criteria_count
        ),
    )
    .await?;

    Ok(CycleDecision::NextCycle)
}

fn completion_satisfied(
    all_done: bool,
    verify_passed: bool,
    done_count: usize,
    criteria_count: usize,
) -> bool {
    if !verify_passed {
        return false;
    }
    if !all_done {
        return false;
    }
    criteria_count == 0 || done_count >= criteria_count
}

#[cfg(test)]
mod tests {
    use super::completion_satisfied;

    #[test]
    fn completion_requires_verify_pass() {
        assert!(!completion_satisfied(true, false, 1, 1));
    }

    #[test]
    fn completion_requires_all_tasks_done() {
        assert!(!completion_satisfied(false, true, 1, 2));
    }

    #[test]
    fn completion_requires_non_empty_tasks() {
        assert!(!completion_satisfied(false, true, 0, 0));
    }

    #[test]
    fn completion_succeeds_with_done_tasks_and_verify_pass() {
        assert!(completion_satisfied(true, true, 2, 2));
    }
}
