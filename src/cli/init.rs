use std::path::Path;

use crate::core::error::OrchaError;
use crate::markdown::template;

/// Execute `orcha init`: scaffold the .orcha/ directory with templates.
pub async fn execute(orch_dir: &Path) -> anyhow::Result<()> {
    if orch_dir.exists() {
        return Err(OrchaError::AlreadyInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    // Create directory structure
    let dirs = [
        orch_dir.to_path_buf(),
        orch_dir.join("roles"),
        orch_dir.join("handoff"),
        orch_dir.join("templates").join("profiles"),
        orch_dir.join("templates").join("roles"),
    ];
    for dir in &dirs {
        tokio::fs::create_dir_all(dir).await?;
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    let default_profile = "cheap_checkpoints";

    // Write all template files
    let files: Vec<(std::path::PathBuf, String)> = vec![
        (orch_dir.join("goal.md"), template::goal_md().to_string()),
        (orch_dir.join("orcha.yml"), template::orcha_yml().to_string()),
        (
            orch_dir.join("status.md"),
            template::status_md(&run_id, default_profile),
        ),
        (
            orch_dir.join("status_log.md"),
            template::status_log_md().to_string(),
        ),
        (
            orch_dir.join("roles").join("planner.md"),
            template::role_planner_md().to_string(),
        ),
        (
            orch_dir.join("roles").join("implementer.md"),
            template::role_implementer_md().to_string(),
        ),
        (
            orch_dir.join("roles").join("reviewer.md"),
            template::role_reviewer_md().to_string(),
        ),
        (
            orch_dir.join("roles").join("verifier.md"),
            template::role_verifier_md().to_string(),
        ),
        (
            orch_dir.join("roles").join("scribe.md"),
            template::role_scribe_md().to_string(),
        ),
        (
            orch_dir.join("handoff").join("inbox.md"),
            template::inbox_md().to_string(),
        ),
        (
            orch_dir.join("handoff").join("outbox.md"),
            template::outbox_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("profiles")
                .join("local_only.md"),
            template::profile_local_only_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("profiles")
                .join("cheap_checkpoints.md"),
            template::profile_cheap_checkpoints_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("profiles")
                .join("quality_gate.md"),
            template::profile_quality_gate_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("profiles")
                .join("unblock_first.md"),
            template::profile_unblock_first_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("planner_backlog.md"),
            template::role_sample_planner_backlog_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("planner_risk_first.md"),
            template::role_sample_planner_risk_first_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("implementer_tdd.md"),
            template::role_sample_implementer_tdd_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("implementer_surgical.md"),
            template::role_sample_implementer_surgical_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("reviewer_security.md"),
            template::role_sample_reviewer_security_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("reviewer_regression.md"),
            template::role_sample_reviewer_regression_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("verifier_fast.md"),
            template::role_sample_verifier_fast_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("verifier_release.md"),
            template::role_sample_verifier_release_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("scribe_compact.md"),
            template::role_sample_scribe_compact_md().to_string(),
        ),
        (
            orch_dir.join("templates")
                .join("roles")
                .join("scribe_handoff.md"),
            template::role_sample_scribe_handoff_md().to_string(),
        ),
    ];

    for (path, content) in &files {
        tokio::fs::write(path, content).await?;
    }

    println!("Initialized orchestration directory at {}", orch_dir.display());
    println!("  Profile: {}", default_profile);
    println!("  Run ID:  {}", run_id);
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {} with your objective",
        orch_dir.join("goal.md").display()
    );
    println!(
        "  2. Edit {} for execution settings",
        orch_dir.join("orcha.yml").display()
    );
    println!("  3. Run `orcha run` to start the first cycle");
    println!(
        "  4. Optional: copy samples from {}",
        orch_dir.join("templates").display()
    );

    Ok(())
}
