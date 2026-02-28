use std::path::Path;

use crate::agent::router::AgentRouter;
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, StopReason, MAX_CYCLES};
use crate::core::profile::ProfileName;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{Task, TaskState};
use crate::machine_config::MachineConfig;

/// Phase 7: Decide
/// Determine next action: next cycle, done, blocked, or escalate.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    _router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    let tasks = status.tasks()?;
    let machine = MachineConfig::load(orch_dir)?;
    let criteria_count = machine.execution.acceptance_criteria.len();

    let verify_passed = status.content.contains("Overall: PASS");
    if completion_satisfied(&tasks, verify_passed, criteria_count) {
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            &format!(
                "Goal achieved. Tasks done: {}/{}; acceptance criteria: {}",
                tasks.iter().filter(|t| t.state == TaskState::Done).count(),
                tasks.len(),
                criteria_count
            ),
        )
        .await?;
        return Ok(CycleDecision::Done);
    }

    let verify_failed = status.content.contains("Overall: FAIL");
    let has_blocked = tasks.iter().any(|t| t.state == TaskState::Blocked);
    let done_count = tasks.iter().filter(|t| t.state == TaskState::Done).count();

    // Check stop conditions
    let next_cycle = status.frontmatter.cycle + 1;
    if next_cycle >= MAX_CYCLES {
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            &format!("Maximum cycles ({}) reached", MAX_CYCLES),
        )
        .await?;
        return Ok(CycleDecision::Blocked(StopReason::MaxCyclesReached));
    }

    if (verify_failed || has_blocked) && status.frontmatter.profile == ProfileName::LocalOnly {
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
            "Continuing to next cycle. Tasks done: {}/{}; verify_passed: {}; blocked: {}; acceptance criteria: {}",
            done_count,
            tasks.len(),
            verify_passed,
            has_blocked,
            criteria_count
        ),
    )
    .await?;

    Ok(CycleDecision::NextCycle)
}

fn completion_satisfied(tasks: &[Task], verify_passed: bool, criteria_count: usize) -> bool {
    if !verify_passed {
        return false;
    }

    let done_count = tasks.iter().filter(|t| t.state == TaskState::Done).count();
    let all_tasks_done = !tasks.is_empty() && done_count == tasks.len();
    if !all_tasks_done {
        return false;
    }

    criteria_count == 0 || done_count >= criteria_count
}

#[cfg(test)]
mod tests {
    use crate::core::task::{Task, TaskState};

    use super::completion_satisfied;

    fn sample_task(id: &str, state: TaskState) -> Task {
        Task {
            id: id.to_string(),
            title: format!("task {}", id),
            state,
            owner: String::new(),
            evidence: String::new(),
            notes: String::new(),
        }
    }

    #[test]
    fn completion_requires_verify_pass() {
        let tasks = vec![sample_task("T1", TaskState::Done)];
        assert!(!completion_satisfied(&tasks, false, 1));
    }

    #[test]
    fn completion_requires_all_tasks_done() {
        let tasks = vec![
            sample_task("T1", TaskState::Done),
            sample_task("T2", TaskState::Todo),
        ];
        assert!(!completion_satisfied(&tasks, true, 2));
    }

    #[test]
    fn completion_requires_non_empty_tasks() {
        let tasks: Vec<Task> = Vec::new();
        assert!(!completion_satisfied(&tasks, true, 0));
    }

    #[test]
    fn completion_succeeds_with_done_tasks_and_verify_pass() {
        let tasks = vec![
            sample_task("T1", TaskState::Done),
            sample_task("T2", TaskState::Done),
        ];
        assert!(completion_satisfied(&tasks, true, 2));
    }
}
