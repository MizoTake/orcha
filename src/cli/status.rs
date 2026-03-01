use std::path::Path;

use chrono::{DateTime, Utc};
use colored::Colorize;
use comfy_table::{Cell, Color, Table};

use crate::core::error::OrchaError;
use crate::core::health::Health;
use crate::core::status::StatusFile;
use crate::core::task::TaskState;
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
    let tasks = status.tasks().unwrap_or_default();

    let health = Health::evaluate(
        &tasks, None, // No verify result available from just reading status
        false,
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
        "  Budget:  {}/{}",
        status.frontmatter.budget.paid_calls_used, status.frontmatter.budget.paid_calls_limit
    );
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
        table.set_header(vec!["ID", "Title", "State", "Owner", "Evidence", "Notes"]);

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
                Cell::new(&t.evidence),
                Cell::new(&t.notes),
            ]);
        }
        println!("{}", table);
    }

    // Summary counts
    let total = tasks.len();
    let done = tasks.iter().filter(|t| t.state == TaskState::Done).count();
    let doing = tasks.iter().filter(|t| t.state == TaskState::Doing).count();
    let blocked = tasks
        .iter()
        .filter(|t| t.state == TaskState::Blocked)
        .count();
    let todo = tasks.iter().filter(|t| t.state == TaskState::Todo).count();

    if total > 0 {
        println!();
        println!(
            "  Progress: {}/{} done, {} doing, {} todo, {} blocked",
            done, total, doing, todo, blocked
        );
    }

    Ok(())
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
