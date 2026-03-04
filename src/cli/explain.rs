use std::path::Path;

use colored::Colorize;

use crate::config::AppConfig;
use crate::core::agent_workspace;
use crate::core::error::OrchaError;
use crate::core::gate;
use crate::core::health::Health;
use crate::core::profile;
use crate::core::status::StatusFile;
use crate::core::task::TaskStore;
use crate::machine_config::MachineConfig;
use crate::machine_config::ProviderMode;

/// Execute `orcha explain`: show current decision reasoning.
pub async fn execute(orch_dir: &Path, config: &AppConfig) -> anyhow::Result<()> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let status = StatusFile::load(&status_path).await?;
    let machine = MachineConfig::load(orch_dir)?;
    let active_profile_ref = machine
        .execution
        .resolve_profile_ref(status.frontmatter.cycle, status.frontmatter.profile);
    let active_profile = active_profile_ref.to_string();
    let task_store = TaskStore::new(orch_dir);
    let task_entries = task_store.list_all().await.unwrap_or_default();
    let tasks: Vec<_> = task_entries.iter().map(|e| e.to_task()).collect();
    let mut profile_rules = machine
        .execution
        .resolve_profile_rules(status.frontmatter.cycle, status.frontmatter.profile);
    if let Some(custom_rules) = profile::load_custom_profile_rules(
        orch_dir,
        active_profile_ref.as_str(),
        status.frontmatter.profile,
    )? {
        profile_rules = custom_rules;
    } else if active_profile_ref.as_profile_name().is_none() {
        anyhow::bail!(
            "Profile '{}' is not built-in and .orcha/profiles/{}.md was not found",
            active_profile_ref.to_string(),
            active_profile_ref.to_string()
        );
    }

    println!("{}", "═══ Decision Reasoning ═══".bold());
    println!();

    // Current state
    println!("{}", "Current State:".bold());
    println!("  Cycle:   {}", status.frontmatter.cycle);
    println!("  Phase:   {}", status.frontmatter.phase);
    println!("  Profile: {}", active_profile);
    if machine.execution.has_profile_strategy() {
        println!("  Profile source: orcha.yml execution.profile + profile_strategy");
    } else if machine.execution.profile.is_some() {
        println!("  Profile source: orcha.yml execution.profile");
    } else {
        println!("  Profile source: status.md frontmatter");
    }
    println!();

    // Profile rules
    println!("{}", "Active Profile Rules:".bold());
    println!("  Default agent:     {}", profile_rules.default_agent);
    if let Some(ref ra) = profile_rules.review_agent {
        println!("  Review agent:      {}", ra);
    }
    if let Some(ref esc) = profile_rules.escalation {
        println!(
            "  Escalation:        after {} failures -> {}",
            esc.failure_threshold, esc.escalate_to
        );
        if let Some(ref cont) = esc.continued_failure_to {
            println!("  Continued failure: -> {}", cont);
        }
    } else {
        println!("  Escalation:        disabled");
    }
    println!(
        "  Security gate:     {}",
        if profile_rules.security_gate_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "  Size gate:         {}",
        if profile_rules.size_gate_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!();

    // Health
    let health = Health::evaluate(&tasks, None, false);
    println!("{}", "Health:".bold());
    println!("  {}", health);
    println!();

    // Available agents
    println!("{}", "Available Agents:".bold());
    println!("  local_llm: always available");
    println!(
        "  claude:    {}",
        if matches!(config.anthropic_mode, ProviderMode::Cli) {
            "available (CLI mode)".green().to_string()
        } else if config.has_anthropic() {
            "available (API key set)".green().to_string()
        } else {
            "not configured".red().to_string()
        }
    );
    println!(
        "  gemini:    {}",
        if matches!(config.gemini_mode, ProviderMode::Cli) {
            "available (CLI mode)".green().to_string()
        } else if config.has_gemini() {
            "available (API key set)".green().to_string()
        } else {
            "not configured".red().to_string()
        }
    );
    println!(
        "  codex:     {}",
        if matches!(config.openai_mode, ProviderMode::Cli) {
            "available (CLI mode)".green().to_string()
        } else if config.has_openai() {
            "available (API key set)".green().to_string()
        } else {
            "not configured".red().to_string()
        }
    );
    println!();

    // Gate evaluation (current state)
    println!("{}", "Gate Status:".bold());
    let size_gate = gate::evaluate_size_gate(0);
    println!("  Size gate:     {:?}", size_gate);
    let security_gate = gate::evaluate_security_gate(None, &[]);
    println!("  Security gate: {:?}", security_gate);
    let unblock_gate = gate::evaluate_unblock_gate(0, &profile_rules);
    println!("  Unblock gate:  {:?}", unblock_gate);
    println!();

    // Budget
    println!("{}", "Budget:".bold());
    println!(
        "  Paid calls: {}/{}",
        status.frontmatter.budget.paid_calls_used, status.frontmatter.budget.paid_calls_limit
    );

    Ok(())
}
