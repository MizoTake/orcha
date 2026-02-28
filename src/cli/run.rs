use std::path::Path;

use colored::Colorize;

use crate::agent::router::AgentRouter;
use crate::config::AppConfig;
use crate::core::cycle::{CycleDecision, Phase, StopReason, MAX_CYCLES};
use crate::core::error::OrchaError;
use crate::core::agent_workspace;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::machine_config::MachineConfig;
use crate::phase;

/// Execute `orcha run`: continue cycles until goal completion or stop condition.
pub async fn execute(orch_dir: &Path, config: &AppConfig) -> anyhow::Result<()> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let mut status = StatusFile::load(&status_path).await?;

    let machine = MachineConfig::load(orch_dir)?;

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

    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let mut terminal_error: Option<anyhow::Error> = None;
    loop {
        // Check stop conditions before each phase step.
        if status.frontmatter.cycle >= MAX_CYCLES {
            terminal_error = Some(
                OrchaError::StopCondition {
                    reason: StopReason::MaxCyclesReached.to_string(),
                }
                .into(),
            );
            break;
        }

        let resolved_profile_name = machine
            .execution
            .resolve_profile_name(status.frontmatter.cycle, status.frontmatter.profile);
        let profile_rules = machine
            .execution
            .resolve_profile_rules(status.frontmatter.cycle, status.frontmatter.profile);
        let router = AgentRouter::new(config, &profile_rules)?;
        status.frontmatter.profile = resolved_profile_name;

        let phase = status.frontmatter.phase;
        println!(
            "{} Cycle {} / Phase: {}",
            "▶".green(),
            status.frontmatter.cycle,
            phase.to_string().yellow()
        );

        status_log::append(
            &log_path,
            &phase.to_string(),
            phase.role_name(),
            "orch",
            &format!("Starting phase: {}", phase),
        )
        .await?;

        let result = match phase {
            Phase::Briefing => phase::briefing::execute(orch_dir, &mut status, &router).await,
            Phase::Plan => phase::plan::execute(orch_dir, &mut status, &router).await,
            Phase::Impl => phase::impl_phase::execute(orch_dir, &mut status, &router).await,
            Phase::Review => phase::review::execute(orch_dir, &mut status, &router).await,
            Phase::Fix => phase::fix::execute(orch_dir, &mut status, &router).await,
            Phase::Verify => phase::verify::execute(orch_dir, &mut status).await,
            Phase::Decide => phase::decide::execute(orch_dir, &mut status, &router).await,
        };

        let mut stop = false;
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
                        stop = true;
                    }
                    CycleDecision::Blocked(reason) => {
                        println!("  {} Blocked: {}", "✗".red(), reason);
                        terminal_error = Some(
                            OrchaError::StopCondition {
                                reason: reason.to_string(),
                            }
                            .into(),
                        );
                        stop = true;
                    }
                    CycleDecision::Escalate(msg) => {
                        println!("  {} Escalation needed: {}", "⚠".yellow(), msg);
                        terminal_error = Some(
                            OrchaError::StopCondition {
                                reason: format!("Escalation needed: {}", msg),
                            }
                            .into(),
                        );
                        stop = true;
                    }
                }

                status_log::append(
                    &log_path,
                    &phase.to_string(),
                    phase.role_name(),
                    "orch",
                    &format!("Phase result: {:?}", decision),
                )
                .await?;
            }
            Err(err) => {
                println!("  {} Phase failed: {}", "✗".red(), err);
                status_log::append(
                    &log_path,
                    &phase.to_string(),
                    phase.role_name(),
                    "orch",
                    &format!("Phase failed: {}", err),
                )
                .await?;
                terminal_error = Some(err);
                stop = true;
            }
        }

        status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
        status.save(&status_path).await?;

        if stop {
            break;
        }
    }

    // Release lock and save final state.
    status.frontmatter.locks.writer = None;
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    status.save(&status_path).await?;

    if let Some(err) = terminal_error {
        return Err(err);
    }

    Ok(())
}
