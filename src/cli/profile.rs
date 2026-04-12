use std::path::Path;

use crate::core::agent_workspace;
use crate::core::error::OrchaError;
use crate::core::profile::ProfileName;
use crate::core::status::StatusFile;
use crate::machine_config::{MachineConfig, ProfileRef};

/// Execute `orcha profile <name>`: change the active profile.
pub async fn execute(orch_dir: &Path, name: &str) -> anyhow::Result<()> {
    let status_path = agent_workspace::resolve_status_path(orch_dir);
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let normalized_name = name.trim().to_lowercase().replace('-', "_");
    let profile = ProfileName::from_str(&normalized_name);
    let custom_profile_path = orch_dir
        .join("profiles")
        .join(format!("{normalized_name}.md"));
    let is_custom_profile = profile.is_none() && custom_profile_path.exists();
    if profile.is_none() && !is_custom_profile {
        return Err(OrchaError::UnknownProfile {
            name: name.to_string(),
        }
        .into());
    }

    let mut status = StatusFile::load(&status_path).await?;
    let old_profile = status.frontmatter.profile;
    if let Some(profile) = profile {
        status.frontmatter.profile = profile;
    }
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    status.save(&status_path).await?;

    let machine_path = MachineConfig::path(orch_dir);
    let mut machine = if machine_path.exists() {
        MachineConfig::load(orch_dir)?
    } else {
        MachineConfig::default()
    };
    machine.execution.profile = Some(match profile {
        Some(profile) => ProfileRef::from(profile),
        None => ProfileRef::new(normalized_name.clone()),
    });
    let yml = serde_yaml::to_string(&machine).map_err(|e| OrchaError::MachineConfigError {
        path: machine_path.clone(),
        reason: e.to_string(),
    })?;
    tokio::fs::write(&machine_path, yml)
        .await
        .map_err(|e| OrchaError::MachineConfigError {
            path: machine_path.clone(),
            reason: e.to_string(),
        })?;

    if let Some(profile) = profile {
        println!("Profile changed: {} -> {}", old_profile, profile);
    } else {
        println!(
            "Profile changed: {} -> {} (custom profile)",
            old_profile, normalized_name
        );
        println!(
            "Status frontmatter profile remains {} (fallback) because custom profiles are resolved from orcha.yml at runtime.",
            old_profile
        );
    }
    println!(
        "Updated {}: execution.profile = {}",
        machine_path.display(),
        normalized_name
    );
    if machine.execution.has_profile_strategy() {
        println!("Note: execution.profile_strategy is active and may override this profile by cycle.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::execute;
    use crate::core::agent_workspace;
    use crate::core::profile::ProfileName;
    use crate::core::status::StatusFile;
    use crate::machine_config::MachineConfig;
    use crate::markdown::template;
    use tempfile::TempDir;

    #[tokio::test]
    async fn execute_updates_status_and_machine_config_for_builtin_profile() {
        let temp = TempDir::new().expect("temp dir should be created");
        let orch_dir = temp.path().join(".orcha");
        let workspace_dir = orch_dir.join("agentworkspace");
        tokio::fs::create_dir_all(&workspace_dir).await.expect("agent workspace should exist");
        let status_path = workspace_dir.join("status.md");
        tokio::fs::write(&status_path, template::status_md("run-1", "cheap_checkpoints")).await.expect("status file should be written");

        execute(&orch_dir, "quality_gate").await.expect("profile change should succeed");

        let status = StatusFile::load(&agent_workspace::resolve_status_path(&orch_dir)).await.expect("status should load");
        assert_eq!(status.frontmatter.profile, ProfileName::QualityGate);
        let machine = MachineConfig::load(&orch_dir).expect("machine config should load");
        assert_eq!(machine.execution.profile.expect("profile ref should exist").as_str(), "quality_gate");
    }

    #[tokio::test]
    async fn execute_keeps_status_profile_for_custom_profile_and_updates_machine_config() {
        let temp = TempDir::new().expect("temp dir should be created");
        let orch_dir = temp.path().join(".orcha");
        let workspace_dir = orch_dir.join("agentworkspace");
        let profiles_dir = orch_dir.join("profiles");
        tokio::fs::create_dir_all(&workspace_dir).await.expect("agent workspace should exist");
        tokio::fs::create_dir_all(&profiles_dir).await.expect("profiles dir should exist");
        let status_path = workspace_dir.join("status.md");
        tokio::fs::write(&status_path, template::status_md("run-1", "cheap_checkpoints")).await.expect("status file should be written");
        tokio::fs::write(profiles_dir.join("my_team_flow.md"), "# Profile: my_team_flow\n").await.expect("custom profile should be written");

        execute(&orch_dir, "my-team-flow").await.expect("custom profile change should succeed");

        let status = StatusFile::load(&agent_workspace::resolve_status_path(&orch_dir)).await.expect("status should load");
        assert_eq!(status.frontmatter.profile, ProfileName::CheapCheckpoints);
        let machine = MachineConfig::load(&orch_dir).expect("machine config should load");
        assert_eq!(machine.execution.profile.expect("profile ref should exist").as_str(), "my_team_flow");
    }
}
