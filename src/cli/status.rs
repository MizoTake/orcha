use std::path::Path;

use chrono::{DateTime, Utc};
use colored::Colorize;
use comfy_table::{Cell, Color, Table};

use crate::core::error::OrchaError;
use crate::core::health::Health;
use crate::core::status::{ReviewStatus, StatusFile, VerifyStatus};
use crate::core::task::{TaskState, TaskStore};
use crate::core::agent_workspace;
use crate::machine_config::MachineConfig;

/// Execute `orcha status`: display status.md dashboard.
pub async fn execute(orch_dir: &Path) -> anyhow::Result<()> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let status = StatusFile::load(&status_path).await?;
    let machine = MachineConfig::load(orch_dir).ok();
    let active_profile = machine
        .as_ref()
        .map(|m| {
            m.execution
                .resolve_profile_ref(status.frontmatter.cycle, status.frontmatter.profile)
                .to_string()
        })
        .unwrap_or_else(|| status.frontmatter.profile.to_string());

    let task_store = TaskStore::new(orch_dir);
    let task_entries = task_store.list_all().await.unwrap_or_default();
    let tasks: Vec<_> = task_entries.iter().map(|e| e.to_task()).collect();

    let health = Health::evaluate(
        &tasks,
        match status.frontmatter.verify_status {
            Some(VerifyStatus::Pass) => Some(true),
            Some(VerifyStatus::Fail) => Some(false),
            _ => None,
        },
        status.frontmatter.review_status == ReviewStatus::IssuesFound,
    );

    // Header
    println!("{}", "═══ Orchestrator Status ═══".bold());
    println!();

    // Overview
    println!("  Run ID:  {}", status.frontmatter.run_id.dimmed());
    println!("  Profile: {}", active_profile.cyan());
    if let Some(machine) = &machine {
        if machine.execution.has_profile_strategy() {
            println!(
                "  Profile source: {}",
                "orcha.yml execution.profile + profile_strategy".dimmed()
            );
        } else if machine.execution.profile.is_some() {
            println!(
                "  Profile source: {}",
                "orcha.yml execution.profile".dimmed()
            );
        }
    }
    println!("  Cycle:   {}", status.frontmatter.cycle.to_string().bold());
    println!(
        "  Phase:   {} {} ({}/{})",
        status.frontmatter.phase.to_string().yellow(),
        status.frontmatter.phase.gauge().dimmed(),
        status.frontmatter.phase.position(),
        crate::core::cycle::Phase::total()
    );
    let age_seconds = last_update_age_seconds(&status.frontmatter.last_update);
    println!(
        "  Last update: {}",
        format_last_update(&status.frontmatter.last_update, age_seconds)
    );
    println!("  Activity: {}", format_activity(age_seconds));
    println!("  Health:  {}", format_health(health));
    println!(
        "  Budget:  {}",
        format_budget(&status)
    );
    if !status.frontmatter.disabled_agents.is_empty() {
        println!("  Disabled agents: {}", format_disabled_agents(&status));
    }
    if let Some(verify_status) = &status.frontmatter.verify_status {
        println!("  Verify: {}", format_verify_status(verify_status));
    }
    if status.frontmatter.review_status == ReviewStatus::IssuesFound {
        println!("  Review: must-fix remaining");
    }
    println!();

    // Lock info
    if let Some(ref writer) = status.frontmatter.locks.writer {
        println!("  {} Writer lock: {}", "⚠".yellow(), writer);
    }
    if let Some(ref task) = status.frontmatter.locks.active_task {
        println!("  Active task: {}", task);
    }

    // Task table
    if tasks.is_empty() {
        println!("  No tasks defined yet.");
    } else {
        println!("{}", "─── Tasks ───".bold());
        let mut table = Table::new();
        table.set_header(vec!["ID", "Title", "State", "Owner"]);

        for t in &tasks {
            let state_color = match t.state {
                TaskState::Todo => Color::White,
                TaskState::Doing => Color::Yellow,
                TaskState::Done => Color::Green,
                TaskState::Blocked => Color::Red,
            };
            table.add_row(vec![
                Cell::new(&t.id),
                Cell::new(&t.title),
                Cell::new(t.state.to_string()).fg(state_color),
                Cell::new(&t.owner),
            ]);
        }
        println!("{}", table);
    }

    // Summary counts
    let total = tasks.len();
    let done = tasks.iter().filter(|t| t.state == TaskState::Done).count();
    let doing = tasks.iter().filter(|t| t.state == TaskState::Doing).count();
    let todo = tasks.iter().filter(|t| t.state == TaskState::Todo).count();
    let blocked = tasks.iter().filter(|t| t.state == TaskState::Blocked).count();

    if total > 0 {
        println!();
        println!(
            "  Progress: {}/{} done, {} doing, {} todo, {} blocked",
            done, total, doing, todo, blocked
        );
    }

    Ok(())
}

fn format_budget(status: &StatusFile) -> String {
    format!(
        "{}/{}",
        status.frontmatter.budget.paid_calls_used, status.frontmatter.budget.paid_calls_limit
    )
}

fn format_disabled_agents(status: &StatusFile) -> String {
    status
        .frontmatter
        .disabled_agents
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_verify_status(status: &VerifyStatus) -> String {
    format!("{status:?}").to_lowercase()
}

fn format_health(health: Health) -> String {
    match health {
        Health::Green => "green".green().bold().to_string(),
        Health::Yellow => "yellow".yellow().bold().to_string(),
        Health::Red => "red".red().bold().to_string(),
    }
}

fn last_update_age_seconds(last_update: &str) -> Option<i64> {
    let parsed = DateTime::parse_from_rfc3339(last_update).ok()?;
    let now = Utc::now();
    let age = now.signed_duration_since(parsed.with_timezone(&Utc)).num_seconds();
    Some(age.max(0))
}

fn format_last_update(last_update: &str, age_seconds: Option<i64>) -> String {
    match age_seconds {
        Some(seconds) => format!("{} ({}s ago)", last_update.dimmed(), seconds),
        None => last_update.dimmed().to_string(),
    }
}

fn format_activity(age_seconds: Option<i64>) -> String {
    match age_seconds {
        Some(seconds) if seconds <= 30 => "active".green().bold().to_string(),
        Some(seconds) if seconds <= 180 => "progressing".cyan().bold().to_string(),
        Some(seconds) if seconds <= 900 => "slow".yellow().bold().to_string(),
        Some(_) => "stale".red().bold().to_string(),
        None => "unknown".white().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_budget, format_disabled_agents, format_verify_status};
    use crate::agent::AgentKind;
    use crate::core::cycle::Phase;
    use crate::core::profile::ProfileName;
    use crate::core::status::{Budget, Locks, ReviewStatus, StatusFile, StatusFrontmatter, VerifyStatus};

    fn sample_status() -> StatusFile {
        StatusFile {
            frontmatter: StatusFrontmatter {
                run_id: "run-1".into(),
                profile: ProfileName::CheapCheckpoints,
                cycle: 2,
                phase: Phase::Review,
                last_update: "2026-01-01T00:00:00Z".into(),
                budget: Budget {
                    paid_calls_used: 1,
                    paid_calls_limit: 3,
                },
                locks: Locks {
                    writer: None,
                    active_task: None,
                },
                review_status: ReviewStatus::Clean,
                verify_status: Some(VerifyStatus::Fail),
                consecutive_verify_failures: 2,
                disabled_agents: vec![AgentKind::Claude, AgentKind::Codex],
            },
            content: String::new(),
        }
    }

    #[test]
    fn budget_and_disabled_agents_are_formatted_for_output() {
        let status = sample_status();
        assert_eq!(format_budget(&status), "1/3");
        assert_eq!(format_disabled_agents(&status), "claude, codex");
    }

    #[test]
    fn verify_status_is_lowercase_for_output() {
        assert_eq!(format_verify_status(&VerifyStatus::Fail), "fail");
    }
}
