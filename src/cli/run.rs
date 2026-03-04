use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::future::Future;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use colored::Colorize;
use serde::Deserialize;
use tokio::process::Command;

use crate::agent::{AgentContext, AgentKind, ContextFile, router::AgentRouter};
use crate::config::AppConfig;
use crate::core::cycle::{CycleDecision, Phase, StopReason};
use crate::core::error::OrchaError;
use crate::core::handoff;
use crate::core::profile;
use crate::core::agent_workspace;
use crate::core::status::StatusFile;
use crate::core::status_log;
use crate::core::task::{Task, TaskState, TaskStore, TaskEntry, TaskFrontmatter, parse_task_table};
use crate::core::structured_log;
use crate::core::workspace_md;
use crate::machine_config::MachineConfig;
use crate::phase;

/// Execute `orcha run`: continue cycles until goal completion or stop condition.
pub async fn execute(
    orch_dir: &Path,
    config: &AppConfig,
    allow_concurrent: bool,
    spec_path: Option<&Path>,
    reset_cycle: bool,
) -> anyhow::Result<()> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let mut status = StatusFile::load(&status_path).await?;
    if reset_cycle {
        reset_status_to_cycle_zero(&mut status);
        status.save(&status_path).await?;
        println!(
            "  {} reset-cycle: status reset to cycle=0 phase=briefing",
            "▶".green()
        );
    }

    let task_store = TaskStore::new(orch_dir);
    task_store.ensure_dirs().await?;

    let mut machine = MachineConfig::load(orch_dir)?;
    let max_cycles = machine.execution.max_cycles;
    let max_consecutive_verify_failures = machine.execution.max_consecutive_verify_failures.max(1);
    let mut consecutive_verify_failures = 0u32;

    // Check stop conditions
    if max_cycles > 0 && status.frontmatter.cycle >= max_cycles {
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
    let mut diff_baseline = collect_git_numstat_snapshot().await;

    if let Some(bootstrap_request) = resolve_bootstrap_request(orch_dir, &task_store, spec_path).await? {
        apply_spec_bootstrap(
            orch_dir,
            &mut status,
            &mut machine,
            config,
            &task_store,
            &bootstrap_request,
        )
        .await?;
        status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
        status.save(&status_path).await?;
    }

    let mut terminal_error: Option<anyhow::Error> = None;
    let mut disabled_agents_by_cli_limit: HashSet<AgentKind> = HashSet::new();
    loop {
        // Check stop conditions before each phase step.
        if max_cycles > 0 && status.frontmatter.cycle >= max_cycles {
            terminal_error = Some(
                OrchaError::StopCondition {
                    reason: StopReason::MaxCyclesReached.to_string(),
                }
                .into(),
            );
            break;
        }

        let (_resolved_profile_ref, resolved_profile_name, profile_rules) =
            resolve_profile_rules_for_cycle(orch_dir, &machine, &status, true)?;
        let router = AgentRouter::new(config, &profile_rules, &disabled_agents_by_cli_limit)?;
        status.frontmatter.profile = resolved_profile_name;
        let status_before_phase = status.clone();
        let phase_timeout = if machine.execution.phase_timeout_seconds == 0 {
            None
        } else {
            Some(Duration::from_secs(machine.execution.phase_timeout_seconds))
        };

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
                    phase::briefing::execute(orch_dir, &mut status, &task_store, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Plan => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::plan::execute(orch_dir, &mut status, &task_store, &router),
                    phase_timeout,
                )
                    .await
            }
            Phase::Impl => {
                execute_phase_with_heartbeat(
                    phase,
                    phase::impl_phase::execute(orch_dir, &mut status, &task_store, &router),
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
                    phase::decide::execute(orch_dir, &mut status, &task_store, &router),
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
                        let completed_cycle = status.frontmatter.cycle;
                        emit_cycle_progress_summary(
                            orch_dir,
                            &log_path,
                            &status,
                            completed_cycle,
                            &mut diff_baseline,
                        )
                        .await?;
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

                        let human_threshold = machine.execution.human_escalation.on_consecutive_failures;
                        if human_threshold > 0 && consecutive_verify_failures >= human_threshold {
                            let reason = format!(
                                "Human escalation requested after {} consecutive verification failures (threshold: {})",
                                consecutive_verify_failures, human_threshold
                            );
                            request_human_intervention(
                                orch_dir,
                                &status,
                                "verify_failure_threshold",
                                &reason,
                                &machine.execution.human_escalation.channel,
                            )
                            .await?;
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
                                "human_escalation",
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
                    } else if consecutive_verify_failures > 0 {
                        consecutive_verify_failures = 0;
                    }

                    if !stop && consecutive_verify_failures >= max_consecutive_verify_failures {
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
                    let human_threshold = machine.execution.human_escalation.on_consecutive_failures;
                    if human_threshold > 0 && human_threshold <= 1 {
                        let reason = format!(
                            "Human escalation requested because phase '{}' failed: {}",
                            phase, err
                        );
                        request_human_intervention(
                            orch_dir,
                            &status,
                            "phase_failure",
                            &reason,
                            &machine.execution.human_escalation.channel,
                        )
                        .await?;
                        append_structured_event(
                            orch_dir,
                            &status,
                            phase,
                            "human_escalation",
                            &reason,
                        )
                        .await;
                    }
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

fn reset_status_to_cycle_zero(status: &mut StatusFile) {
    status.frontmatter.cycle = 0;
    status.frontmatter.phase = Phase::Briefing;
    status.frontmatter.locks.active_task = None;
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
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

#[derive(Debug, Deserialize)]
struct SpecBootstrapPayload {
    goal_summary: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    verification_commands: Vec<String>,
    #[serde(default)]
    ambiguities: Vec<String>,
}

#[derive(Debug)]
struct ParsedSpecBootstrap {
    goal_summary: String,
    acceptance_criteria: Vec<String>,
    verification_commands: Vec<String>,
    ambiguities: Vec<String>,
    tasks: Vec<Task>,
}

#[derive(Debug, Clone)]
struct CycleDiffFileDelta {
    path: String,
    added: i64,
    deleted: i64,
}

#[derive(Debug, Default)]
struct CycleDiffSummary {
    total_added: i64,
    total_deleted: i64,
    changed_files: Vec<CycleDiffFileDelta>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpecBootstrapMode {
    FullSync,
    TaskOnly,
}

#[derive(Debug)]
struct SpecBootstrapRequest {
    source_path: PathBuf,
    mode: SpecBootstrapMode,
}

fn resolve_profile_rules_for_cycle(
    orch_dir: &Path,
    machine: &MachineConfig,
    status: &StatusFile,
    print_custom_profile_notice: bool,
) -> anyhow::Result<(
    crate::machine_config::ProfileRef,
    crate::core::profile::ProfileName,
    crate::core::profile::ProfileRules,
)> {
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
        if print_custom_profile_notice {
            println!(
                "  {} Using profile rules from .orcha/profiles/{}.md",
                "✓".green(),
                resolved_profile_ref.to_string().cyan()
            );
        }
    } else if resolved_profile_ref.as_profile_name().is_none() {
        anyhow::bail!(
            "Profile '{}' is not built-in and .orcha/profiles/{}.md was not found",
            resolved_profile_ref.to_string(),
            resolved_profile_ref.to_string()
        );
    }

    Ok((resolved_profile_ref, resolved_profile_name, profile_rules))
}

async fn resolve_bootstrap_request(
    orch_dir: &Path,
    task_store: &TaskStore,
    spec_path: Option<&Path>,
) -> anyhow::Result<Option<SpecBootstrapRequest>> {
    if let Some(spec_path) = spec_path {
        return Ok(Some(SpecBootstrapRequest {
            source_path: resolve_spec_path(spec_path)?,
            mode: SpecBootstrapMode::FullSync,
        }));
    }

    if task_store.is_empty().await? {
        return Ok(Some(SpecBootstrapRequest {
            source_path: orch_dir.join("goal.md"),
            mode: SpecBootstrapMode::TaskOnly,
        }));
    }

    Ok(None)
}

async fn apply_spec_bootstrap(
    orch_dir: &Path,
    status: &mut StatusFile,
    machine: &mut MachineConfig,
    config: &AppConfig,
    task_store: &TaskStore,
    request: &SpecBootstrapRequest,
) -> anyhow::Result<()> {
    let resolved_spec_path = request.source_path.clone();
    let spec_content = tokio::fs::read_to_string(&resolved_spec_path).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to read spec file '{}': {}",
            resolved_spec_path.display(),
            e
        )
    })?;
    let spec_name = resolved_spec_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("spec.md");
    let goal_path = orch_dir.join("goal.md");
    let current_goal = tokio::fs::read_to_string(&goal_path)
        .await
        .unwrap_or_else(|_| String::new());

    let (_, _, profile_rules) =
        resolve_profile_rules_for_cycle(orch_dir, machine, status, false)?;
    let disabled_agents: HashSet<AgentKind> = HashSet::new();
    let router = AgentRouter::new(config, &profile_rules, &disabled_agents)?;

    let instruction = match request.mode {
        SpecBootstrapMode::FullSync => full_spec_bootstrap_instruction().to_string(),
        SpecBootstrapMode::TaskOnly => task_breakdown_instruction().to_string(),
    };
    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: spec_name.to_string(),
                content: spec_content,
            },
            ContextFile {
                name: "goal.md".to_string(),
                content: current_goal,
            },
            ContextFile {
                name: "status.md".to_string(),
                content: status.content.clone(),
            },
        ],
        role: "planner".to_string(),
        instruction,
    };

    let bootstrap_label = match request.mode {
        SpecBootstrapMode::FullSync => "spec bootstrap",
        SpecBootstrapMode::TaskOnly => "task breakdown",
    };
    println!("  {} {}: {}", "▶".green(), bootstrap_label, resolved_spec_path.display());
    let response = router.default_agent().respond(&context).await?;
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        match request.mode {
            SpecBootstrapMode::FullSync => "spec_bootstrap",
            SpecBootstrapMode::TaskOnly => "task_breakdown",
        },
        "planner",
        &response.model_used,
        &response.content,
    )
    .await?;

    let log_path = agent_workspace::resolve_status_log_path(orch_dir);
    match request.mode {
        SpecBootstrapMode::FullSync => {
            let parsed = parse_spec_bootstrap_response(&response.content)?;
            let new_goal = render_goal_from_spec(spec_name, &parsed);
            tokio::fs::write(&goal_path, new_goal).await?;

            if !parsed.acceptance_criteria.is_empty() {
                machine.execution.acceptance_criteria = parsed.acceptance_criteria.clone();
            }
            if !parsed.verification_commands.is_empty() {
                machine.execution.verification.commands = parsed.verification_commands.clone();
            }

            let machine_path = MachineConfig::path(orch_dir);
            let machine_yaml = serde_yaml::to_string(machine)?;
            tokio::fs::write(&machine_path, machine_yaml).await?;

            if !parsed.tasks.is_empty() {
                create_task_files_from_tasks(task_store, &parsed.tasks).await?;
            }

            let applied_summary = format!(
                "Spec bootstrap applied from {} (criteria: {}, verify commands: {}, tasks: {}, ambiguities: {})",
                spec_name,
                machine.execution.acceptance_criteria.len(),
                machine.execution.verification.commands.len(),
                parsed.tasks.len(),
                parsed.ambiguities.len()
            );
            status_log::append(
                &log_path,
                "briefing",
                "scribe",
                &response.model_used,
                &applied_summary,
            )
            .await?;
            append_structured_event(
                orch_dir,
                status,
                Phase::Briefing,
                "spec_bootstrap_applied",
                &applied_summary,
            )
            .await;

            if machine.execution.human_escalation.on_ambiguous_spec && !parsed.ambiguities.is_empty()
            {
                let reason = format!(
                    "Specification contains ambiguities requiring human input:\n- {}",
                    parsed.ambiguities.join("\n- ")
                );
                request_human_intervention(
                    orch_dir,
                    status,
                    "ambiguous_spec",
                    &reason,
                    &machine.execution.human_escalation.channel,
                )
                .await?;
                status_log::append(
                    &log_path,
                    "briefing",
                    "scribe",
                    "orch",
                    "Spec bootstrap stopped for human clarification",
                )
                .await?;
                return Err(OrchaError::StopCondition {
                    reason: "Ambiguous specification requires human input".to_string(),
                }
                .into());
            }
        }
        SpecBootstrapMode::TaskOnly => {
            let tasks = parse_task_breakdown_response(
                &response.content,
                &machine.execution.acceptance_criteria,
            )?;
            if !tasks.is_empty() {
                create_task_files_from_tasks(task_store, &tasks).await?;
            }
            let applied_summary = format!(
                "Task breakdown initialized from {} (tasks: {})",
                spec_name,
                tasks.len()
            );
            status_log::append(
                &log_path,
                "plan",
                "planner",
                &response.model_used,
                &applied_summary,
            )
            .await?;
            append_structured_event(
                orch_dir,
                status,
                Phase::Plan,
                "task_breakdown_applied",
                &applied_summary,
            )
            .await;
        }
    }

    Ok(())
}

fn full_spec_bootstrap_instruction() -> &'static str {
    "Analyze the provided specification and output exactly two parts:\n\
1) A single JSON fenced code block with keys:\n\
   - goal_summary: string\n\
   - acceptance_criteria: string[]\n\
   - verification_commands: string[]\n\
   - ambiguities: string[]\n\
2) A markdown task table in this exact format:\n\
| ID | Title | State | Owner | Evidence | Notes |\n\
|---|---|---|---|---|---|\n\
| T1 | Task title | issue | implementer | | reason |\n\
\n\
Rules:\n\
- Keep acceptance_criteria concrete and testable.\n\
- verification_commands must be runnable shell commands.\n\
- Put uncertain points in ambiguities.\n\
- State must be issue for all initial tasks."
}

fn task_breakdown_instruction() -> &'static str {
    "Break down the provided goal/spec into an initial task plan.\n\
Return a markdown task table in this exact format:\n\
| ID | Title | State | Owner | Evidence | Notes |\n\
|---|---|---|---|---|---|\n\
| T1 | Task title | issue | implementer | | reason |\n\
\n\
Rules:\n\
- Create atomic tasks that can be completed in short cycles.\n\
- Set State to issue for all rows.\n\
- Keep IDs sequential as T1, T2, T3..."
}

fn parse_task_breakdown_response(
    response: &str,
    fallback_criteria: &[String],
) -> anyhow::Result<Vec<Task>> {
    if let Ok(tasks) = parse_task_table(response) {
        if !tasks.is_empty() {
            return Ok(tasks);
        }
    }

    let fallback = tasks_from_criteria(fallback_criteria);
    if fallback.is_empty() {
        anyhow::bail!("Task breakdown response did not contain a valid task table");
    }

    Ok(fallback)
}

fn resolve_spec_path(spec_path: &Path) -> anyhow::Result<PathBuf> {
    if spec_path.is_absolute() {
        return Ok(spec_path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(spec_path))
}

fn parse_spec_bootstrap_response(response: &str) -> anyhow::Result<ParsedSpecBootstrap> {
    let json_block = extract_fenced_block(response, "json").ok_or_else(|| {
        anyhow::anyhow!(
            "Spec bootstrap response must include a JSON fenced block with goal_summary/acceptance_criteria/verification_commands/ambiguities"
        )
    })?;
    let payload: SpecBootstrapPayload = serde_json::from_str(&json_block).map_err(|e| {
        anyhow::anyhow!("Failed to parse spec bootstrap JSON block: {}", e)
    })?;

    let acceptance_criteria = normalize_non_empty_lines(&payload.acceptance_criteria);
    let verification_commands = normalize_non_empty_lines(&payload.verification_commands);
    let ambiguities = normalize_non_empty_lines(&payload.ambiguities);
    let tasks = match parse_task_table(response) {
        Ok(parsed) if !parsed.is_empty() => parsed,
        _ => tasks_from_criteria(&acceptance_criteria),
    };

    Ok(ParsedSpecBootstrap {
        goal_summary: payload.goal_summary.trim().to_string(),
        acceptance_criteria,
        verification_commands,
        ambiguities,
        tasks,
    })
}

fn extract_fenced_block(raw: &str, language: &str) -> Option<String> {
    let mut in_block = false;
    let mut matches_language = false;
    let mut lines = Vec::new();
    let lang = language.to_ascii_lowercase();

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if !in_block {
                in_block = true;
                let declared = rest.trim().to_ascii_lowercase();
                matches_language = declared.is_empty() || declared == lang;
                continue;
            }
            if in_block && matches_language {
                return Some(lines.join("\n"));
            }
            in_block = false;
            matches_language = false;
            lines.clear();
            continue;
        }

        if in_block && matches_language {
            lines.push(line.to_string());
        }
    }

    None
}

fn normalize_non_empty_lines(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

fn tasks_from_criteria(criteria: &[String]) -> Vec<Task> {
    criteria
        .iter()
        .enumerate()
        .map(|(idx, criterion)| Task {
            id: format!("T{}", idx + 1),
            title: sanitize_markdown_cell(criterion),
            state: TaskState::Issue,
            owner: "implementer".to_string(),
            evidence: String::new(),
            notes: "Generated from spec acceptance criteria".to_string(),
        })
        .collect()
}

async fn create_task_files_from_tasks(task_store: &TaskStore, tasks: &[Task]) -> anyhow::Result<()> {
    for task in tasks {
        let file_name = TaskEntry::generate_file_name(&task.id, &task.title);
        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: task.id.clone(),
                title: task.title.clone(),
                owner: task.owner.clone(),
                created: chrono::Utc::now().to_rfc3339(),
            },
            content: format!(
                "## Description\n\n{}\n\n## Evidence\n\n\n\n## Notes\n\n{}\n",
                task.title, task.notes
            ),
            state: TaskState::Issue,
            file_name,
        };
        task_store.create_task(&entry).await?;
    }
    Ok(())
}

fn sanitize_markdown_cell(value: &str) -> String {
    value.replace('|', "/").trim().to_string()
}

fn render_goal_from_spec(spec_name: &str, parsed: &ParsedSpecBootstrap) -> String {
    let mut out = String::new();
    out.push_str("# Goal\n\n");
    out.push_str("## Background\n\n");
    if parsed.goal_summary.is_empty() {
        out.push_str("- Generated from spec bootstrap.\n");
    } else {
        out.push_str(&parsed.goal_summary);
        out.push('\n');
    }
    out.push_str("\n## Acceptance Criteria\n\n");
    if parsed.acceptance_criteria.is_empty() {
        out.push_str("- [ ] Define acceptance criteria from specification\n");
    } else {
        for criterion in &parsed.acceptance_criteria {
            out.push_str(&format!("- [ ] {}\n", criterion));
        }
    }
    out.push_str("\n## Constraints\n\n");
    out.push_str(&format!("- Source spec: {}\n", spec_name));
    if !parsed.ambiguities.is_empty() {
        out.push_str("- Ambiguities to clarify:\n");
        for ambiguity in &parsed.ambiguities {
            out.push_str(&format!("  - {}\n", ambiguity));
        }
    }
    out.push_str("\n## Verification Commands\n\n");
    out.push_str("Execution commands are defined in `orcha.yml` under:\n\n");
    out.push_str("```yaml\nexecution:\n  verification:\n    commands:\n");
    for command in &parsed.verification_commands {
        out.push_str(&format!("      - \"{}\"\n", command.replace('\"', "\\\"")));
    }
    out.push_str("```\n\n");
    out.push_str("## Quality Priority\n\n");
    out.push_str("quality\n");
    out
}

async fn request_human_intervention(
    orch_dir: &Path,
    status: &StatusFile,
    trigger: &str,
    reason: &str,
    channel: &str,
) -> anyhow::Result<()> {
    let inbox_path = workspace_md::resolve_handoff_file(orch_dir, "inbox")?;
    let message = format!(
        "## Human Escalation Required\n\n- trigger: {}\n- channel: {}\n- run_id: {}\n- cycle: {}\n- phase: {}\n\n{}\n",
        trigger,
        channel,
        status.frontmatter.run_id,
        status.frontmatter.cycle,
        status.frontmatter.phase,
        reason
    );
    handoff::append_handoff(&inbox_path, "orcha", &message).await?;
    println!(
        "  {} Human escalation ({}) queued: {}",
        "⚠".yellow(),
        channel,
        inbox_path.display()
    );
    Ok(())
}

async fn emit_cycle_progress_summary(
    orch_dir: &Path,
    log_path: &Path,
    status: &StatusFile,
    cycle: u32,
    diff_baseline: &mut Option<BTreeMap<String, (i64, i64)>>,
) -> anyhow::Result<()> {
    let Some(current_snapshot) = collect_git_numstat_snapshot().await else {
        return Ok(());
    };
    let summary = build_cycle_diff_summary(diff_baseline.as_ref(), &current_snapshot);
    *diff_baseline = Some(current_snapshot);

    let (pass_count, fail_count) = parse_verify_status_counts(&status.content);
    let total_verify = pass_count + fail_count;
    let verify_part = if total_verify > 0 {
        format!(" / verify: {}/{} pass", pass_count, total_verify)
    } else {
        String::new()
    };
    let top_files = summary
        .changed_files
        .iter()
        .take(3)
        .map(|d| format!("{} ({:+}/{:+})", d.path, d.added, d.deleted))
        .collect::<Vec<_>>()
        .join(", ");
    let detail_part = if top_files.is_empty() {
        String::new()
    } else {
        format!(" / {}", top_files)
    };
    let message = format!(
        "Cycle {} diff: Δ+{} Δ-{} ({} files){}{}",
        cycle,
        summary.total_added,
        summary.total_deleted,
        summary.changed_files.len(),
        verify_part,
        detail_part
    );
    println!("  {} {}", "ℹ".cyan(), message);
    status_log::append(log_path, "decide", "planner", "orch", &message).await?;
    append_structured_event(orch_dir, status, Phase::Decide, "cycle_summary", &message).await;
    Ok(())
}

async fn collect_git_numstat_snapshot() -> Option<BTreeMap<String, (i64, i64)>> {
    let output = Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Some(parse_git_numstat_snapshot(&stdout))
}

fn parse_git_numstat_snapshot(raw: &str) -> BTreeMap<String, (i64, i64)> {
    let mut snapshot = BTreeMap::new();
    for line in raw.lines() {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let added = parse_numstat_value(parts[0]);
        let deleted = parse_numstat_value(parts[1]);
        let path = parts[2].trim();
        if path.is_empty() {
            continue;
        }
        snapshot.insert(path.to_string(), (added, deleted));
    }
    snapshot
}

fn parse_numstat_value(raw: &str) -> i64 {
    raw.trim().parse::<i64>().unwrap_or(0)
}

fn build_cycle_diff_summary(
    baseline: Option<&BTreeMap<String, (i64, i64)>>,
    current: &BTreeMap<String, (i64, i64)>,
) -> CycleDiffSummary {
    let mut summary = CycleDiffSummary::default();
    let mut union_keys = baseline
        .map(|map| map.keys().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();
    union_keys.extend(current.keys().cloned());

    for key in union_keys {
        let (base_added, base_deleted) = baseline
            .and_then(|m| m.get(&key).copied())
            .unwrap_or((0, 0));
        let (current_added, current_deleted) = current.get(&key).copied().unwrap_or((0, 0));
        let delta_added = current_added - base_added;
        let delta_deleted = current_deleted - base_deleted;
        if delta_added == 0 && delta_deleted == 0 {
            continue;
        }
        summary.total_added += delta_added;
        summary.total_deleted += delta_deleted;
        summary.changed_files.push(CycleDiffFileDelta {
            path: key,
            added: delta_added,
            deleted: delta_deleted,
        });
    }

    summary
        .changed_files
        .sort_by(|a, b| (b.added.abs() + b.deleted.abs()).cmp(&(a.added.abs() + a.deleted.abs())));
    summary
}

fn parse_verify_status_counts(content: &str) -> (usize, usize) {
    let pass = content.matches("Status: PASS").count();
    let fail = content.matches("Status: FAIL").count();
    (pass, fail)
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
        build_cycle_diff_summary, clear_writer_lock_if_matches, detect_limit_reached_cli_agent,
        lock_id_for_pid, parse_git_numstat_snapshot, parse_lock_pid, parse_spec_bootstrap_response,
        parse_task_breakdown_response, parse_verify_status_counts, process_exists,
        reset_status_to_cycle_zero,
        release_writer_lock_for_pid, resolve_bootstrap_request, SpecBootstrapMode,
    };
    use crate::agent::AgentKind;
    use crate::core::{agent_workspace, status::StatusFile};
    use crate::core::task::TaskStore;
    use std::collections::BTreeMap;
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
    fn reset_status_to_cycle_zero_sets_briefing_and_clears_active_task() {
        let mut status = StatusFile::from_str(
            "---\nrun_id: test-001\nprofile: cheap_checkpoints\ncycle: 3\nphase: review\nlast_update: '2025-01-01T00:00:00Z'\nbudget:\n  paid_calls_used: 1\n  paid_calls_limit: 10\nlocks:\n  writer: orch-9999\n  active_task: T3\n---\n\n## Goal\n\nBuild the thing.\n",
        )
        .expect("status should parse");

        reset_status_to_cycle_zero(&mut status);

        assert_eq!(status.frontmatter.cycle, 0);
        assert_eq!(status.frontmatter.phase.to_string(), "briefing");
        assert_eq!(status.frontmatter.locks.active_task, None);
        assert_eq!(status.frontmatter.locks.writer, Some("orch-9999".to_string()));
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

    #[test]
    fn parse_spec_bootstrap_response_extracts_json_and_tasks() {
        let response = r#"
```json
{
  "goal_summary": "Implement todo library",
  "acceptance_criteria": ["Create task", "List tasks"],
  "verification_commands": ["cargo test"],
  "ambiguities": ["Persistence format is unspecified"]
}
```

| ID | Title | State | Owner | Evidence | Notes |
|---|---|---|---|---|---|
| T1 | Add create API | issue | implementer | | from spec |
| T2 | Add list API | issue | implementer | | from spec |
"#;

        let parsed = parse_spec_bootstrap_response(response).expect("parse should succeed");
        assert_eq!(parsed.goal_summary, "Implement todo library");
        assert_eq!(parsed.acceptance_criteria.len(), 2);
        assert_eq!(parsed.verification_commands, vec!["cargo test"]);
        assert_eq!(parsed.ambiguities.len(), 1);
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].id, "T1");
    }

    #[test]
    fn parse_git_numstat_snapshot_reads_numstat_lines() {
        let raw = "10\t2\tsrc/main.rs\n3\t1\tREADME.md\n";
        let parsed = parse_git_numstat_snapshot(raw);
        assert_eq!(parsed.get("src/main.rs"), Some(&(10, 2)));
        assert_eq!(parsed.get("README.md"), Some(&(3, 1)));
    }

    #[test]
    fn build_cycle_diff_summary_computes_delta_from_baseline() {
        let mut baseline = BTreeMap::new();
        baseline.insert("src/main.rs".to_string(), (10, 2));
        let mut current = BTreeMap::new();
        current.insert("src/main.rs".to_string(), (13, 5));
        current.insert("src/lib.rs".to_string(), (4, 0));

        let summary = build_cycle_diff_summary(Some(&baseline), &current);
        assert_eq!(summary.total_added, 7);
        assert_eq!(summary.total_deleted, 3);
        assert_eq!(summary.changed_files.len(), 2);
    }

    #[test]
    fn parse_verify_status_counts_counts_pass_and_fail() {
        let content = "Status: PASS\nStatus: FAIL\nStatus: PASS\n";
        let (pass, fail) = parse_verify_status_counts(content);
        assert_eq!(pass, 2);
        assert_eq!(fail, 1);
    }

    #[tokio::test]
    async fn resolve_bootstrap_request_defaults_to_goal_when_tasks_missing() {
        let temp = TempDir::new().expect("temp dir should be created");
        let task_store = TaskStore::new(temp.path());
        task_store.ensure_dirs().await.unwrap();

        let request = resolve_bootstrap_request(temp.path(), &task_store, None)
            .await
            .expect("should resolve")
            .expect("bootstrap should be enabled");
        assert_eq!(request.mode, SpecBootstrapMode::TaskOnly);
        assert!(request.source_path.ends_with("goal.md"));
    }

    #[tokio::test]
    async fn resolve_bootstrap_request_uses_fullsync_when_spec_is_provided() {
        let temp = TempDir::new().expect("temp dir should be created");
        let task_store = TaskStore::new(temp.path());
        task_store.ensure_dirs().await.unwrap();

        let request = resolve_bootstrap_request(
            temp.path(),
            &task_store,
            Some(std::path::Path::new("requirements.md")),
        )
        .await
        .expect("should resolve")
        .expect("bootstrap should be enabled");
        assert_eq!(request.mode, SpecBootstrapMode::FullSync);
        assert!(request.source_path.ends_with("requirements.md"));
    }

    #[tokio::test]
    async fn resolve_bootstrap_request_skips_when_tasks_exist() {
        let temp = TempDir::new().expect("temp dir should be created");
        let task_store = TaskStore::new(temp.path());
        task_store.ensure_dirs().await.unwrap();

        // Create a task file so tasks are not empty
        use crate::core::task::{TaskEntry, TaskFrontmatter, TaskState};
        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id: "T1".into(),
                title: "Setup".into(),
                owner: "implementer".into(),
                created: String::new(),
            },
            content: String::new(),
            state: TaskState::Issue,
            file_name: "T1-setup.md".into(),
        };
        task_store.create_task(&entry).await.unwrap();

        let request = resolve_bootstrap_request(temp.path(), &task_store, None)
            .await
            .expect("should resolve");
        assert!(request.is_none());
    }

    #[test]
    fn parse_task_breakdown_response_falls_back_to_criteria() {
        let tasks = parse_task_breakdown_response("no table here", &["criterion a".to_string()])
            .expect("fallback should build tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "T1");
    }

    #[test]
    fn parse_task_breakdown_response_uses_table_when_present() {
        let response = "| ID | Title | State | Owner | Evidence | Notes |\n|---|---|---|---|---|---|\n| T1 | Implement API | issue | implementer | | |\n";
        let tasks = parse_task_breakdown_response(response, &[])
            .expect("should parse table");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Implement API");
    }

    #[test]
    fn parse_task_breakdown_response_errors_when_no_table_and_no_fallback() {
        let err = parse_task_breakdown_response("no table here", &[])
            .expect_err("should fail");
        assert!(err
            .to_string()
            .contains("did not contain a valid task table"));
    }

    #[test]
    fn parse_spec_bootstrap_response_uses_criteria_for_tasks_when_table_missing() {
        let response = r#"
```json
{
  "goal_summary": "Implement todo library",
  "acceptance_criteria": ["Create task", "Create task", "List tasks"],
  "verification_commands": ["cargo test"],
  "ambiguities": []
}
```
"#;

        let parsed = parse_spec_bootstrap_response(response).expect("parse should succeed");
        assert_eq!(
            parsed.acceptance_criteria,
            vec!["Create task".to_string(), "List tasks".to_string()]
        );
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].id, "T1");
        assert_eq!(parsed.tasks[1].id, "T2");
    }
}
