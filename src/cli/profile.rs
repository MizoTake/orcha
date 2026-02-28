use std::path::Path;

use crate::core::error::OrchaError;
use crate::core::profile::ProfileName;
use crate::core::status::StatusFile;
use crate::machine_config::MachineConfig;

/// Execute `orcha profile <name>`: change the active profile.
pub async fn execute(orch_dir: &Path, name: &str) -> anyhow::Result<()> {
    let status_path = orch_dir.join("status.md");
    if !status_path.exists() {
        return Err(OrchaError::NotInitialized {
            path: orch_dir.to_path_buf(),
        }
        .into());
    }

    let profile = ProfileName::from_str(name).ok_or_else(|| OrchaError::UnknownProfile {
        name: name.to_string(),
    })?;

    let mut status = StatusFile::load(&status_path).await?;
    let old_profile = status.frontmatter.profile;
    status.frontmatter.profile = profile;
    status.frontmatter.last_update = chrono::Utc::now().to_rfc3339();
    status.save(&status_path).await?;

    let machine_path = MachineConfig::path(orch_dir);
    let mut machine = if machine_path.exists() {
        MachineConfig::load(orch_dir)?
    } else {
        MachineConfig::default()
    };
    machine.execution.profile = Some(profile);
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

    println!("Profile changed: {} -> {}", old_profile, profile);
    println!(
        "Updated {}: execution.profile = {}",
        machine_path.display(),
        profile
    );

    Ok(())
}
