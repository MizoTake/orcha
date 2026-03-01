use std::path::Path;
use std::collections::HashSet;

use colored::Colorize;

use crate::agent::{AgentKind, router::AgentRouter};
use crate::config::AppConfig;
use crate::core::cycle::{CycleDecision, Phase, StopReason, MAX_CYCLES};
use crate::core::error::OrchaError;
use crate::core::agent_workspace;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::machine_config::MachineConfig;
use crate::phase;

/// Execute `orcha run`: continue cycles until goal completion or stop condition.
pub async fn execute(
    orch_dir: &Path,
    config: &AppConfig,
    allow_concurrent: bool,
) -> anyhow::Result<()> {
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

    // Check writer lock and acquire lock unless concurrent mode is requested.
    if !allow_concurrent {
        if let Some(ref writer) = status.frontmatter.locks.writer {
            return Err(OrchaError::LockConflict {
                holder: writer.clone(),
            }
            .into());
        }

        let lock_id = format!("orch-{}", std::process::id());
        status.frontmatter.locks.writer = Some(lock_id);
        status.save(&status_path).await?;
    }

    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let mut terminal_error: Option<anyhow::Error> = None;
    let mut disabled_agents_by_cli_limit: HashSet<AgentKind> = HashSet::new();
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
        let router = AgentRouter::new(config, &profile_rules, &disabled_agents_by_cli_limit)?;
        status.frontmatter.profile = resolved_profile_name;
        let status_before_phase = status.clone();

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
        let mut retry_phase = false;
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
                let limited_agent = if machine.execution.cli_limit.disable_agent_on_limit {
                    detect_limit_reached_cli_agent(&err)
                } else {
                    None
                };

                if let Some(agent_kind) = limited_agent {
                    if disabled_agents_by_cli_limit.insert(agent_kind) {
                        println!(
                            "  {} {} limit detected. Disabling for this run and retrying phase.",
                            "⚠".yellow(),
                            agent_kind.to_string().yellow()
                        );
                        status_log::append(
                            &log_path,
                            &phase.to_string(),
                            phase.role_name(),
                            "orch",
                            &format!(
                                "CLI limit detected for {}; disabled for this run and retrying phase",
                                agent_kind
                            ),
                        )
                        .await?;
                        // Revert in-memory phase-side effects before retrying.
                        status = status_before_phase;
                        retry_phase = true;
                    }
                }

                if !retry_phase {
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
        }

        status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
        status.save(&status_path).await?;

        if retry_phase {
            continue;
        }
        if stop {
            break;
        }
    }

    // Release lock and save final state.
    if !allow_concurrent {
        status.frontmatter.locks.writer = None;
    }
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    status.save(&status_path).await?;

    if let Some(err) = terminal_error {
        return Err(err);
    }

    Ok(())
}

fn detect_limit_reached_cli_agent(err: &anyhow::Error) -> Option<AgentKind> {
    let msg = err.to_string().to_lowercase();
    if !msg.contains("local cli '") {
        return None;
    }
    if !is_limit_message(&msg) {
        return None;
    }

    if msg.contains("local cli 'claude")
        || msg.contains("local cli 'claude-code")
        || msg.contains("local cli 'claudecode")
    {
        return Some(AgentKind::Claude);
    }
    if msg.contains("local cli 'codex") {
        return Some(AgentKind::Codex);
    }
    None
}

fn is_limit_message(msg: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "rate limit",
        "rate-limit",
        "limit reached",
        "quota exceeded",
        "quota",
        "too many requests",
        "429",
        "exceeded your current quota",
        "usage limit",
    ];
    NEEDLES.iter().any(|needle| msg.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::detect_limit_reached_cli_agent;
    use crate::agent::AgentKind;

    #[test]
    fn detect_limit_for_claude_cli_error() {
        let err = anyhow::anyhow!(
            "Local CLI 'claude' failed with exit code Some(1): 429 rate limit exceeded"
        );
        assert_eq!(detect_limit_reached_cli_agent(&err), Some(AgentKind::Claude));
    }

    #[test]
    fn detect_limit_for_codex_cli_error() {
        let err =
            anyhow::anyhow!("Local CLI 'codex' failed with exit code Some(1): quota exceeded");
        assert_eq!(detect_limit_reached_cli_agent(&err), Some(AgentKind::Codex));
    }

    #[test]
    fn ignore_non_limit_cli_error() {
        let err = anyhow::anyhow!("Local CLI 'codex' failed with exit code Some(1): syntax error");
        assert_eq!(detect_limit_reached_cli_agent(&err), None);
    }
}
