use std::path::Path;

use colored::Colorize;
use comfy_table::{Cell, Color, Table};

use crate::core::error::OrchaError;
use crate::core::health::Health;
use crate::core::status::StatusFile;
use crate::core::task::TaskState;
use crate::machine_config::MachineConfig;

/// Execute `orcha status`: display status.md dashboard.
pub async fn execute(orch_dir: &Path) -> anyhow::Result<()> {
    let status_path = orch_dir.join("status.md");
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let status = StatusFile::load(&status_path).await?;
    let machine_profile = MachineConfig::load(orch_dir)
        .ok()
        .and_then(|m| m.execution.profile);
    let active_profile = machine_profile.unwrap_or(status.frontmatter.profile);
    let tasks = status.tasks().unwrap_or_default();

    let health = Health::evaluate(
        &tasks,
        None, // No verify result available from just reading status
        false,
    );

    // Header
    println!("{}", "═══ Orchestrator Status ═══".bold());
    println!();

    // Overview
    println!(
        "  Run ID:  {}",
        status.frontmatter.run_id.dimmed()
    );
    println!(
        "  Profile: {}",
        active_profile.to_string().cyan()
    );
    if machine_profile.is_some() {
        println!("  Profile source: {}", "orcha.yml execution.profile".dimmed());
    }
    println!(
        "  Cycle:   {}",
        status.frontmatter.cycle.to_string().bold()
    );
    println!(
        "  Phase:   {}",
        status.frontmatter.phase.to_string().yellow()
    );
    println!(
        "  Health:  {}",
        format_health(health)
    );
    println!(
        "  Budget:  {}/{}",
        status.frontmatter.budget.paid_calls_used,
        status.frontmatter.budget.paid_calls_limit
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
