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
    let blocked_tasks = all_tasks
        .iter()
        .filter(|t| t.state == TaskState::Blocked)
        .map(|t| t.frontmatter.id.clone())
        .collect::<Vec<_>>();

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

    if !blocked_tasks.is_empty() {
        let reason = format!(
            "{}: {}",
            StopReason::BlockedTasksRequireIntervention,
            blocked_tasks.join(", ")
        );
        status_log::append(
            &log_path,
            "decide",
            "planner",
            "orch",
            &reason,
        )
        .await?;
        return Ok(CycleDecision::Blocked(
            StopReason::BlockedTasksRequireIntervention,
        ));
    }

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
    use tempfile::TempDir;

    use super::completion_satisfied;
    use super::execute;
    use crate::agent::router::AgentRouter;
    use crate::config::AppConfig;
    use crate::core::status::{Budget, Locks, ReviewStatus, StatusFile, StatusFrontmatter};
    use crate::core::task::{TaskEntry, TaskFrontmatter, TaskState, TaskStore};
    use crate::machine_config::MachineConfig;

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

    #[tokio::test]
    async fn blocked_tasks_stop_decision() {
        let temp = TempDir::new().unwrap();
        let task_store = TaskStore::new(temp.path());
        task_store.ensure_dirs().await.unwrap();
        std::fs::write(
            temp.path().join("orcha.yml"),
            serde_yaml::to_string(&MachineConfig::default()).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(temp.path().join("agentworkspace")).unwrap();
        std::fs::write(temp.path().join("agentworkspace").join("status_log.md"), "# Status Log\n")
            .unwrap();

        let blocked = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: "T1".into(),
                title: "Blocked work".into(),
                owner: "implementer".into(),
                created: String::new(),
            },
            content: String::new(),
            state: TaskState::Blocked,
            file_name: "T1-blocked-work.md".into(),
        };
        task_store.create_task(&blocked).await.unwrap();

        let status = &mut StatusFile {
            frontmatter: StatusFrontmatter {
                run_id: "run-1".into(),
                profile: crate::core::profile::ProfileName::CheapCheckpoints,
                cycle: 0,
                phase: crate::core::cycle::Phase::Decide,
                last_update: chrono::Utc::now().to_rfc3339(),
                budget: Budget {
                    paid_calls_used: 0,
                    paid_calls_limit: 10,
                },
                locks: Locks {
                    writer: None,
                    active_task: None,
                },
                review_status: ReviewStatus::Clean,
                verify_status: Some(crate::core::status::VerifyStatus::Pass),
                consecutive_verify_failures: 0,
                disabled_agents: vec![],
            },
            content: String::new(),
        };

        let router = AgentRouter::new(
            &AppConfig::from_env(),
            &crate::core::profile::ProfileRules::from_name(
                crate::core::profile::ProfileName::CheapCheckpoints,
            ),
            &std::collections::HashSet::new(),
        )
        .unwrap();

        let decision = execute(temp.path(), status, &task_store, &router).await.unwrap();
        assert_eq!(
            decision,
            crate::core::cycle::CycleDecision::Blocked(
                crate::core::cycle::StopReason::BlockedTasksRequireIntervention
            )
        );
    }
}
