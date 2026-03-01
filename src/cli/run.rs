use std::path::Path;
use std::collections::HashSet;
use std::future::Future;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use colored::Colorize;
use tokio::process::Command;

use crate::agent::{AgentKind, router::AgentRouter};
use crate::config::AppConfig;
use crate::core::cycle::{CycleDecision, Phase, StopReason};
use crate::core::error::OrchaError;
use crate::core::profile;
use crate::core::agent_workspace;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::structured_log;
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
    let max_cycles = machine.execution.max_cycles.max(1);
    let max_consecutive_verify_failures = machine.execution.max_consecutive_verify_failures.max(1);
    let phase_timeout = if machine.execution.phase_timeout_seconds == 0 {
        None
    } else {
        Some(Duration::from_secs(machine.execution.phase_timeout_seconds))
    };
    let mut consecutive_verify_failures = 0u32;

    // Check stop conditions
    if status.frontmatter.cycle >= max_cycles {
        return Err(OrchaError::StopCondition {
            reason: StopReason::MaxCyclesReached.to_string(),
        }
        .into());
    }

    // Check writer lock and acquire lock unless concurrent mode is requested.
    if !allow_concurrent {
        if let Some(writer) = status.frontmatter.locks.writer.clone() {
            if is_stale_writer_lock(&writer).await {
                println!(
                    "  {} Clearing stale lock: {}",
                    "⚠".yellow(),
                    writer
                );
                status.frontmatter.locks.writer = None;
                status.save(&status_path).await?;
            } else {
                return Err(OrchaError::LockConflict {
                    holder: writer.clone(),
                }
                .into());
            }
        }

        let lock_id = lock_id_for_pid(std::process::id());
        status.frontmatter.locks.writer = Some(lock_id);
        status.save(&status_path).await?;
    }

    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let mut terminal_error: Option<anyhow::Error> = None;
    let mut disabled_agents_by_cli_limit: HashSet<AgentKind> = HashSet::new();
    loop {
        // Check stop conditions before each phase step.
        if status.frontmatter.cycle >= max_cycles {
            terminal_error = Some(
                OrchaError::StopCondition {
                    reason: StopReason::MaxCyclesReached.to_string(),
                }
                .into(),
            );
            break;
        }

        let resolved_profile_ref = machine
            .execution
            .resolve_profile_ref(status.frontmatter.cycle, status.frontmatter.profile);
        let resolved_profile_name = resolved_profile_ref
            .as_profile_name()
            .unwrap_or(status.frontmatter.profile);
        let mut profile_rules = machine
            .execution
            .resolve_profile_rules(status.frontmatter.cycle, status.frontmatter.profile);
        if let Some(file_rules) = profile::load_custom_profile_rules(
            orch_dir,
            resolved_profile_ref.as_str(),
            resolved_profile_name,
        )? {
            profile_rules = file_rules;
            println!(
                "  {} Using profile rules from .orcha/profiles/{}.md",
                "✓".green(),
                resolved_profile_ref.to_string().cyan()
            );
        } else if resolved_profile_ref.as_profile_name().is_none() {
            anyhow::bail!(
                "Profile '{}' is not built-in and .orcha/profiles/{}.md was not found",
                resolved_profile_ref.to_string(),
                resolved_profile_ref.to_string()
            );
        }
        let router = AgentRouter::new(config, &profile_rules, &disabled_agents_by_cli_limit)?;
        status.frontmatter.profile = resolved_profile_name;
        let status_before_phase = status.clone();

        let phase = status.frontmatter.phase;
        status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
        status.save(&status_path).await?;
        println!(
            "{} Cycle {} / Phase: {} {} ({}/{})",
            "▶".green(),
            status.frontmatter.cycle,
            phase.to_string().yellow(),
            phase.gauge().dimmed(),
            phase.position(),
            Phase::total()
        );

        status_log::append(
            &log_path,
            &phase.to_string(),
            phase.role_name(),
            "orch",
            &format!("Starting phase: {}", phase),
        )
        .await?;
        append_structured_event(
            orch_dir,
            &status,
            phase,
            "phase_start",
            &format!("Starting phase: {}", phase),
        )
        .await;

        let result = match phase {
            Phase::Briefing => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::briefing::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Plan => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::plan::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Impl => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::impl_phase::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Review => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::review::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Fix => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::fix::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Verify => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::verify::execute(orch_dir, &mut status),
                    phase_timeout,
                )
                    .await
            }
            Phase::Decide => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::decide::execute(orch_dir, &mut status, &router),
                    phase_timeout,
                )
                    .await
            }
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
                append_structured_event(
                    orch_dir,
                    &status,
                    phase,
                    "phase_result",
                    &format!("{:?}", decision),
                )
                .await;

                if phase == Phase::Verify {
                    let verify_failed = status.content.contains("Overall: FAIL");
                    if verify_failed {
                        consecutive_verify_failures = consecutive_verify_failures.saturating_add(1);
                        let detail = format!(
                            "Verification failed consecutively: {}/{}",
                            consecutive_verify_failures, max_consecutive_verify_failures
                        );
                        println!("  {} {}", "⚠".yellow(), detail);
                        status_log::append(
                            &log_path,
                            "verify",
                            "verifier",
                            "orch",
                            &detail,
                        )
                        .await?;
                        append_structured_event(
                            orch_dir,
                            &status,
                            phase,
                            "verify_failed",
                            &detail,
                        )
                        .await;
                    } else if consecutive_verify_failures > 0 {
                        consecutive_verify_failures = 0;
                    }

                    if consecutive_verify_failures >= max_consecutive_verify_failures {
                        let reason = format!(
                            "Verification failed {} times consecutively (limit: {})",
                            consecutive_verify_failures, max_consecutive_verify_failures
                        );
                        status_log::append(
                            &log_path,
                            "decide",
                            "planner",
                            "orch",
                            &reason,
                        )
                        .await?;
                        append_structured_event(
                            orch_dir,
                            &status,
                            phase,
                            "stop_condition",
                            &reason,
                        )
                        .await;
                        terminal_error = Some(
                            OrchaError::StopCondition {
                                reason,
                            }
                            .into(),
                        );
                        stop = true;
                    }
                }
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
                        append_structured_event(
                            orch_dir,
                            &status,
                            phase,
                            "phase_retry",
                            &format!("CLI limit detected for {}; retrying phase", agent_kind),
                        )
                        .await;
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
                    append_structured_event(
                        orch_dir,
                        &status,
                        phase,
                        "phase_failed",
                        &err.to_string(),
                    )
                    .await;
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

async fn execute_phase_with_heartbeat<F>(
    phase: Phase,
    phase_future: F,
    phase_timeout: Option<Duration>,
) -> anyhow::Result<CycleDecision>
where
    F: Future<Output = anyhow::Result<CycleDecision>>,
{
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
    let started_at = Instant::now();

    print_inline_status(&format!(
        "  {} {} 実行中... (role: {})",
        "…".dimmed(),
        phase.to_string().yellow(),
        phase.role_name()
    ));

    tokio::pin!(phase_future);
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // First tick fires immediately; consume it so heartbeat starts after interval.
    heartbeat.tick().await;

    loop {
        tokio::select! {
            result = &mut phase_future => {
                finish_inline_status(&format!(
                    "  {} {} 完了 ({}s)",
                    "✓".green(),
                    phase.to_string().yellow(),
                    started_at.elapsed().as_secs()
                ));
                return result;
            }
            _ = heartbeat.tick() => {
                if let Some(timeout) = phase_timeout {
                    if started_at.elapsed() >= timeout {
                        finish_inline_status(&format!(
                            "  {} {} timeout ({}s >= {}s)",
                            "✗".red(),
                            phase.to_string().yellow(),
                            started_at.elapsed().as_secs(),
                            timeout.as_secs()
                        ));
                        anyhow::bail!(
                            "Phase '{}' timed out after {} seconds",
                            phase,
                            timeout.as_secs()
                        );
                    }
                }
                print_inline_status(&format!(
                    "  {} {} 実行中... {}s 経過",
                    "…".dimmed(),
                    phase.to_string().yellow(),
                    started_at.elapsed().as_secs()
                ));
            }
        }
    }
}

async fn append_structured_event(
    orch_dir: &Path,
    status: &StatusFile,
    phase: Phase,
    event: &str,
    message: &str,
) {
    if let Err(err) = structured_log::append(orch_dir, status, phase, event, message).await {
        eprintln!("  ⚠ structured log write failed: {}", err);
    }
}

fn print_inline_status(message: &str) {
    print!("\r\x1b[2K{}", message);
    let _ = io::stdout().flush();
}

fn finish_inline_status(message: &str) {
    print_inline_status(message);
    println!();
}

pub async fn release_writer_lock_for_current_process(orch_dir: &Path) -> anyhow::Result<bool> {
    release_writer_lock_for_pid(orch_dir, std::process::id()).await
}

async fn release_writer_lock_for_pid(orch_dir: &Path, pid: u32) -> anyhow::Result<bool> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Ok(false);
    }

    let mut status = StatusFile::load(&status_path).await?;
    if clear_writer_lock_if_matches(&mut status, &lock_id_for_pid(pid)) {
        status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
        status.save(&status_path).await?;
        return Ok(true);
    }
    Ok(false)
}

fn clear_writer_lock_if_matches(status: &mut StatusFile, expected_lock_id: &str) -> bool {
    if status.frontmatter.locks.writer.as_deref() != Some(expected_lock_id) {
        return false;
    }
    status.frontmatter.locks.writer = None;
    true
}

fn lock_id_for_pid(pid: u32) -> String {
    format!("orch-{pid}")
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

async fn is_stale_writer_lock(writer: &str) -> bool {
    let Some(pid) = parse_lock_pid(writer) else {
        return false;
    };
    !process_exists(pid).await
}

fn parse_lock_pid(writer: &str) -> Option<u32> {
    writer.strip_prefix("orch-")?.parse::<u32>().ok()
}

#[cfg(windows)]
async fn process_exists(pid: u32) -> bool {
    let filter = format!("PID eq {}", pid);
    let output = Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .await;
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        return false;
    }
    let lower = stdout.to_ascii_lowercase();
    if lower.contains("no tasks are running") {
        return false;
    }
    stdout.contains(&format!("\"{}\"", pid))
}

#[cfg(not(windows))]
async fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .await
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::{
        clear_writer_lock_if_matches, detect_limit_reached_cli_agent, lock_id_for_pid, parse_lock_pid,
        process_exists, release_writer_lock_for_pid,
    };
    use crate::agent::AgentKind;
    use crate::core::{agent_workspace, status::StatusFile};
    use tempfile::TempDir;

    fn sample_status_with_writer(writer: &str) -> String {
        format!(
            "---\nrun_id: test-001\nprofile: cheap_checkpoints\ncycle: 1\nphase: plan\nlast_update: '2025-01-01T00:00:00Z'\nbudget:\n  paid_calls_used: 0\n  paid_calls_limit: 10\nlocks:\n  writer: {}\n  active_task: null\n---\n\n## Goal\n\nBuild the thing.\n",
            writer
        )
    }

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

    #[test]
    fn parse_lock_pid_accepts_orch_lock_format() {
        assert_eq!(parse_lock_pid("orch-1234"), Some(1234));
        assert_eq!(parse_lock_pid("orch-abc"), None);
        assert_eq!(parse_lock_pid("other-1234"), None);
    }

    #[test]
    fn clear_writer_lock_only_when_lock_matches() {
        let mut status = StatusFile::from_str(&sample_status_with_writer("orch-1234"))
            .expect("status should parse");
        assert!(clear_writer_lock_if_matches(&mut status, "orch-1234"));
        assert_eq!(status.frontmatter.locks.writer, None);

        let mut status = StatusFile::from_str(&sample_status_with_writer("orch-1234"))
            .expect("status should parse");
        assert!(!clear_writer_lock_if_matches(&mut status, "orch-9876"));
        assert_eq!(
            status.frontmatter.locks.writer,
            Some("orch-1234".to_string())
        );
    }

    #[tokio::test]
    async fn release_writer_lock_for_pid_updates_status_file() {
        let temp = TempDir::new().expect("temp dir should be created");
        let workspace = agent_workspace::dir(temp.path());
        std::fs::create_dir_all(&workspace).expect("workspace should be created");
        let status_path = workspace.join("status.md");
        std::fs::write(&status_path, sample_status_with_writer("orch-4321"))
            .expect("status should be written");

        let released = release_writer_lock_for_pid(temp.path(), 4321)
            .await
            .expect("release should succeed");
        assert!(released);

        let saved = StatusFile::load(&status_path)
            .await
            .expect("status should load");
        assert_eq!(saved.frontmatter.locks.writer, None);
    }

    #[tokio::test]
    async fn release_writer_lock_for_pid_ignores_other_owner() {
        let temp = TempDir::new().expect("temp dir should be created");
        let workspace = agent_workspace::dir(temp.path());
        std::fs::create_dir_all(&workspace).expect("workspace should be created");
        let status_path = workspace.join("status.md");
        std::fs::write(&status_path, sample_status_with_writer("orch-1234"))
            .expect("status should be written");

        let released = release_writer_lock_for_pid(temp.path(), 9999)
            .await
            .expect("release should succeed");
        assert!(!released);

        let saved = StatusFile::load(&status_path)
            .await
            .expect("status should load");
        assert_eq!(
            saved.frontmatter.locks.writer,
            Some(lock_id_for_pid(1234))
        );
    }

    #[tokio::test]
    async fn process_exists_detects_current_process() {
        assert!(process_exists(std::process::id()).await);
    }
}
