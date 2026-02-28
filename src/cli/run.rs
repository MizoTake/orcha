use std::path::Path;

use colored::Colorize;

use crate::agent::router::AgentRouter;
use crate::config::AppConfig;
use crate::core::cycle::{CycleDecision, Phase, StopReason, MAX_CYCLES};
use crate::core::error::OrchaError;
use crate::core::profile::ProfileRules;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::phase;

/// Execute `orcha run`: run one step from the current phase.
pub async fn execute(orch_dir: &Path, config: &AppConfig) -> anyhow::Result<()> {
    let status_path = orch_dir.join("status.md");
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let mut status = StatusFile::load(&status_path).await?;

    // Check stop conditions
    if status.frontmatter.cycle >= MAX_CYCLES {
        return Err(OrchaError::StopCondition {
            reason: StopReason::MaxCyclesReached.to_string(),
        }
        .into());
    }

    // Check writer lock
    if let Some(ref writer) = status.frontmatter.locks.writer {
        return Err(OrchaError::LockConflict {
            holder: writer.clone(),
        }
        .into());
    }

    // Acquire lock
    let lock_id = format!("orch-{}", std::process::id());
    status.frontmatter.locks.writer = Some(lock_id.clone());
    status.save(&status_path).await?;

    let phase = status.frontmatter.phase;
    let profile_rules = ProfileRules::from_name(status.frontmatter.profile);
    let router = AgentRouter::new(config, &profile_rules)?;

    println!(
        "{} Cycle {} / Phase: {}",
        "▶".green(),
        status.frontmatter.cycle,
        phase.to_string().yellow()
    );

    // Log phase start
    let log_path = orch_dir.join("status_log.md");
    status_log::append(
        &log_path,
        &phase.to_string(),
        phase.role_name(),
        "orch",
        &format!("Starting phase: {}", phase),
    )
    .await?;

    // Execute the current phase
    let result = match phase {
        Phase::Briefing => phase::briefing::execute(orch_dir, &mut status, &router).await,
        Phase::Plan => phase::plan::execute(orch_dir, &mut status, &router).await,
        Phase::Impl => phase::impl_phase::execute(orch_dir, &mut status, &router).await,
        Phase::Review => phase::review::execute(orch_dir, &mut status, &router).await,
        Phase::Fix => phase::fix::execute(orch_dir, &mut status, &router).await,
        Phase::Verify => phase::verify::execute(orch_dir, &mut status).await,
        Phase::Decide => phase::decide::execute(orch_dir, &mut status, &router).await,
    };

    // Handle phase result
    match result {
        Ok(decision) => {
            match &decision {
                CycleDecision::NextPhase => {
                    status.advance_phase();
                    println!(
                        "  {} -> Next phase: {}",
                        "✓".green(),
                        status.frontmatter.phase.to_string().yellow()
                    );
                }
                CycleDecision::NextCycle => {
                    status.start_new_cycle();
                    println!(
                        "  {} -> Starting cycle {}",
                        "↻".cyan(),
                        status.frontmatter.cycle
                    );
                }
                CycleDecision::Done => {
                    println!("  {} Goal achieved!", "✓".green().bold());
                }
                CycleDecision::Blocked(reason) => {
                    println!("  {} Blocked: {}", "✗".red(), reason);
                }
                CycleDecision::Escalate(msg) => {
                    println!("  {} Escalation needed: {}", "⚠".yellow(), msg);
                }
            }

            // Log result
            status_log::append(
                &log_path,
                &phase.to_string(),
                phase.role_name(),
                "orch",
                &format!("Phase result: {:?}", decision),
            )
            .await?;
        }
        Err(e) => {
            println!("  {} Phase failed: {}", "✗".red(), e);
            status_log::append(
                &log_path,
                &phase.to_string(),
                phase.role_name(),
                "orch",
                &format!("Phase failed: {}", e),
            )
            .await?;
        }
    }

    // Release lock and save
    status.frontmatter.locks.writer = None;
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    status.save(&status_path).await?;

    Ok(())
}
