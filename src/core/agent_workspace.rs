use std::path::{Path, PathBuf};

pub const AGENT_WORKSPACE_DIR: &str = "agentworkspace";
pub const STATUS_FILE_NAME: &str = "status.md";
pub const STATUS_LOG_FILE_NAME: &str = "status_log.md";

pub fn dir(orch_dir: &Path) -> PathBuf {
    orch_dir.join(AGENT_WORKSPACE_DIR)
}

pub fn status_path(orch_dir: &Path) -> PathBuf {
    dir(orch_dir).join(STATUS_FILE_NAME)
}

pub fn status_log_path(orch_dir: &Path) -> PathBuf {
    dir(orch_dir).join(STATUS_LOG_FILE_NAME)
}

pub fn resolve_status_path(orch_dir: &Path) -> PathBuf {
    let preferred = status_path(orch_dir);
    if preferred.exists() {
        return preferred;
    }

    let legacy = orch_dir.join(STATUS_FILE_NAME);
    if legacy.exists() {
        legacy
    } else {
        preferred
    }
}

pub fn resolve_status_log_path(orch_dir: &Path) -> PathBuf {
    let preferred = status_log_path(orch_dir);
    if preferred.exists() {
        return preferred;
    }

    let legacy = orch_dir.join(STATUS_LOG_FILE_NAME);
    if legacy.exists() {
        legacy
    } else {
        preferred
    }
}

pub async fn write_response(
    orch_dir: &Path,
    cycle: u32,
    phase: &str,
    role: &str,
    model: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let workspace = dir(orch_dir);
    tokio::fs::create_dir_all(&workspace).await?;

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = format!(
        "cycle{:03}_{}_{}_{}.md",
        cycle,
        sanitize(phase),
        sanitize(role),
        timestamp
    );
    let path = workspace.join(file_name);

    let body = format!(
        "---\ncycle: {}\nphase: {}\nrole: {}\nmodel: {}\ntime: {}\n---\n\n{}",
        cycle,
        phase,
        role,
        model,
        chrono::Utc::now().to_rfc3339(),
        content
    );
    tokio::fs::write(&path, body).await?;
    Ok(path)
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn resolve_status_prefers_agentworkspace_file() {
        let temp = TempDir::new().unwrap();
        let preferred = status_path(temp.path());
        std::fs::create_dir_all(preferred.parent().unwrap()).unwrap();
        std::fs::write(&preferred, "ok").unwrap();
        std::fs::write(temp.path().join(STATUS_FILE_NAME), "legacy").unwrap();

        assert_eq!(resolve_status_path(temp.path()), preferred);
    }

    #[test]
    fn resolve_status_falls_back_to_legacy() {
        let temp = TempDir::new().unwrap();
        let legacy = temp.path().join(STATUS_FILE_NAME);
        std::fs::write(&legacy, "legacy").unwrap();

        assert_eq!(resolve_status_path(temp.path()), legacy);
    }

    #[test]
    fn resolve_status_defaults_to_agentworkspace_path_when_missing() {
        let temp = TempDir::new().unwrap();

        assert_eq!(resolve_status_path(temp.path()), status_path(temp.path()));
    }

    #[test]
    fn resolve_status_log_falls_back_to_legacy() {
        let temp = TempDir::new().unwrap();
        let legacy = temp.path().join(STATUS_LOG_FILE_NAME);
        std::fs::write(&legacy, "legacy").unwrap();

        assert_eq!(resolve_status_log_path(temp.path()), legacy);
    }
}
