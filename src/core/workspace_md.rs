use std::path::{Path, PathBuf};

/// Resolve a role markdown file from `.orcha/roles/`.
///
/// Selection order:
/// 1) exact stem match (`<preferred>.md`)
/// 2) filename containing preferred key
/// 3) first markdown file in lexical order
pub fn resolve_role_file(orch_dir: &Path, preferred: &str) -> anyhow::Result<PathBuf> {
    let roles_dir = orch_dir.join("roles");
    resolve_markdown_file(&roles_dir, preferred)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No role markdown found in {} (preferred: {})",
            roles_dir.display(),
            preferred
        )
    })
}

/// Resolve a handoff markdown file from `.orcha/handoff/`.
///
/// Selection order:
/// 1) exact stem match (`<preferred>.md`)
/// 2) filename containing preferred key
/// 3) first markdown file in lexical order
/// 4) fallback path `<preferred>.md` (for first-time creation)
pub fn resolve_handoff_file(orch_dir: &Path, preferred: &str) -> anyhow::Result<PathBuf> {
    let handoff_dir = orch_dir.join("handoff");
    Ok(
        resolve_markdown_file(&handoff_dir, preferred)?
            .unwrap_or_else(|| handoff_dir.join(format!("{}.md", normalize_key(preferred)))),
    )
}

fn resolve_markdown_file(dir: &Path, preferred: &str) -> anyhow::Result<Option<PathBuf>> {
    let entries = list_markdown_files(dir)?;
    if entries.is_empty() {
        return Ok(None);
    }

    let preferred_key = normalize_key(preferred);

    if let Some(found) = entries
        .iter()
        .find(|p| file_stem_key(p).as_deref() == Some(preferred_key.as_str()))
    {
        return Ok(Some(found.clone()));
    }

    if let Some(found) = entries.iter().find(|p| {
        file_stem_key(p)
            .as_deref()
            .is_some_and(|stem| stem.contains(&preferred_key))
    }) {
        return Ok(Some(found.clone()));
    }

    Ok(entries.into_iter().next())
}

fn list_markdown_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(dir).map_err(|e| {
        anyhow::anyhow!("Failed to read directory {}: {}", dir.display(), e)
    })? {
        let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if is_md {
            files.push(path);
        }
    }

    files.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default()
    });
    Ok(files)
}

fn file_stem_key(path: &Path) -> Option<String> {
    Some(normalize_key(path.file_stem()?.to_str()?))
}

fn normalize_key(raw: &str) -> String {
    raw.trim().to_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{resolve_handoff_file, resolve_role_file};

    #[test]
    fn resolve_role_file_prefers_exact_name() {
        let dir = tempdir().expect("tempdir");
        let roles = dir.path().join("roles");
        fs::create_dir_all(&roles).expect("create roles");
        fs::write(roles.join("planner.md"), "").expect("write planner");
        fs::write(roles.join("planner_fast.md"), "").expect("write planner_fast");

        let path = resolve_role_file(dir.path(), "planner").expect("resolve role");
        assert!(path.ends_with("planner.md"));
    }

    #[test]
    fn resolve_role_file_falls_back_to_partial_match() {
        let dir = tempdir().expect("tempdir");
        let roles = dir.path().join("roles");
        fs::create_dir_all(&roles).expect("create roles");
        fs::write(roles.join("scribe_compact.md"), "").expect("write scribe");
        fs::write(roles.join("planner.md"), "").expect("write planner");

        let path = resolve_role_file(dir.path(), "scribe").expect("resolve role");
        assert!(path.ends_with("scribe_compact.md"));
    }

    #[test]
    fn resolve_handoff_file_falls_back_to_existing_markdown() {
        let dir = tempdir().expect("tempdir");
        let handoff = dir.path().join("handoff");
        fs::create_dir_all(&handoff).expect("create handoff");
        fs::write(handoff.join("shared.md"), "").expect("write shared");

        let path = resolve_handoff_file(dir.path(), "outbox").expect("resolve handoff");
        assert!(path.ends_with("shared.md"));
    }

    #[test]
    fn resolve_handoff_file_uses_default_name_when_empty() {
        let dir = tempdir().expect("tempdir");
        let handoff = dir.path().join("handoff");
        fs::create_dir_all(&handoff).expect("create handoff");

        let path = resolve_handoff_file(dir.path(), "inbox").expect("resolve handoff");
        assert!(path.ends_with("inbox.md"));
    }
}
