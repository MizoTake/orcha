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
