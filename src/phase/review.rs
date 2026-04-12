use std::path::Path;

use crate::agent::router::{AgentRouter, GateContext};
use crate::agent::{AgentContext, ContextFile};
use crate::core::agent_workspace;
use crate::core::cycle::{CycleDecision, Phase};
use crate::core::status::{ReviewStatus, StatusFile};
use crate::core::task::{TaskEntry, TaskFrontmatter, TaskState, TaskStore};
use crate::core::status_log;
use crate::core::workspace_md;

/// Phase 4: Review
/// Reviewer agent reviews the latest git commit.
/// If must-fix issues are found they are filed as issues for the next planning cycle.
pub async fn execute(
    orch_dir: &Path,
    status: &mut StatusFile,
    task_store: &TaskStore,
    router: &AgentRouter,
) -> anyhow::Result<CycleDecision> {
    let log_path = agent_workspace::resolve_status_log_path(orch_dir);

    let role_path = workspace_md::resolve_role_file(orch_dir, "reviewer")?;
    let role_name = role_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("reviewer.md")
        .to_string();
    let role = tokio::fs::read_to_string(&role_path).await?;

    // Review only the latest commit (impl phase commits its changes)
    let diff = get_latest_commit_diff().await;
    let diff_lines = diff.as_ref().map(|d| d.lines().count()).unwrap_or(0);

    let context = AgentContext {
        context_files: vec![
            ContextFile {
                name: "status.md".into(),
                content: status.content.clone(),
            },
            ContextFile {
                name: role_name,
                content: role,
            },
            ContextFile {
                name: "latest_commit".into(),
                content: diff
                    .clone()
                    .unwrap_or_else(|| "No commit diff available.".into()),
            },
        ],
        role: "reviewer".to_string(),
        instruction: "Review the latest commit shown in latest_commit. Provide findings:\n\
             ```\n\
             Findings: High / Med / Low\n\
             Must-fix:\n\
             - item 1  (or '- (none)' if no blockers)\n\
             paid_review_required: yes/no\n\
             reason: explanation\n\
             ```"
            .to_string(),
    };

    let gate_ctx = GateContext {
        diff_content: diff,
        diff_lines,
        file_paths: latest_commit_file_paths().await,
        consecutive_verify_failures: status.frontmatter.consecutive_verify_failures,
    };

    let agent = router.select(Phase::Review, &gate_ctx);
    let response = agent.respond(&context).await?;
    if response.is_paid {
        status.frontmatter.budget.paid_calls_used =
            status.frontmatter.budget.paid_calls_used.saturating_add(1);
    }
    crate::core::agent_workspace::write_response(
        orch_dir,
        status.frontmatter.cycle,
        "review",
        "reviewer",
        &response.model_used,
        &response.content,
    )
    .await?;

    // Parse review findings
    let has_must_fix = response.content.contains("Must-fix:")
        && !response.content.contains("Must-fix:\n- (none)")
        && !response.content.contains("Must-fix:\nNone");

    let needs_paid = response.content.contains("paid_review_required: yes");

    // If critical issues found: file them as an open task for the next cycle
    if has_must_fix {
        let title = format!("Review findings (cycle {})", status.frontmatter.cycle);
        let description = extract_must_fix_section(&response.content);
        let all_tasks = task_store.list_all().await.unwrap_or_default();
        let review_task_count = all_tasks
            .iter()
            .filter(|t| t.frontmatter.id.starts_with('R'))
            .count();
        let id = format!("R{}", review_task_count + 1);
        let file_name = TaskEntry::generate_file_name(&id, &title);
        let entry = TaskEntry {
            frontmatter: TaskFrontmatter {
                id,
                title,
                owner: String::new(),
                created: chrono::Utc::now().to_rfc3339(),
            },
            content: format!("## Description\n\n{}\n\n## Evidence\n\n\n\n## Notes\n\n\n", description),
            state: TaskState::Open,
            file_name,
        };
        if let Err(e) = task_store.create_task(&entry).await {
            eprintln!("  ⚠ Failed to create open task from review findings: {}", e);
        } else {
            status_log::append(
                &log_path,
                "review",
                "reviewer",
                &response.model_used,
                "Must-fix items filed as open task for next cycle",
            )
            .await?;
        }
    }

    // Review always sets Clean — issues are tracked in the issue store, not the fix phase
    status.frontmatter.review_status = ReviewStatus::Clean;

    status_log::append(
        &log_path,
        "review",
        "reviewer",
        &response.model_used,
        &format!(
            "Review completed. Must-fix: {}, Paid review needed: {}",
            has_must_fix, needs_paid
        ),
    )
    .await?;

    // Store review output in status notes
    let review_note = format!(
        "## Review (Cycle {})\n\n{}",
        status.frontmatter.cycle, &response.content
    );
    update_latest_notes(&mut status.content, &review_note);

    if needs_paid && !response.is_paid {
        return Ok(CycleDecision::Escalate(
            "Reviewer recommends paid review".into(),
        ));
    }

    Ok(CycleDecision::NextPhase)
}

/// Extract must-fix items from the review response for issue filing.
fn extract_must_fix_section(response: &str) -> String {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in response.lines() {
        if line.trim_start().starts_with("Must-fix:") {
            in_section = true;
            lines.push(line.to_string());
            continue;
        }
        if in_section {
            // Stop at next key or end of block
            if line.trim().is_empty()
                || line.contains(':')
                    && !line.trim_start().starts_with('-')
                    && !line.trim_start().starts_with('*')
            {
                break;
            }
            lines.push(line.to_string());
        }
    }
    if lines.is_empty() {
        response.to_string()
    } else {
        lines.join("\n")
    }
}

/// Return the diff of the latest git commit (git show HEAD).
async fn get_latest_commit_diff() -> Option<String> {
    let output = tokio::process::Command::new("git")
        .args(["show", "HEAD"])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() { None } else { Some(diff) }
    } else {
        None
    }
}

/// Return the list of files changed in the latest commit.
async fn latest_commit_file_paths() -> Vec<String> {
    let output = tokio::process::Command::new("git")
        .args(["show", "--name-only", "--format=", "HEAD"])
        .output()
        .await;
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn update_latest_notes(content: &mut String, note: &str) {
    if let Some(pos) = content.find("## Latest Notes") {
        let after_start = (pos + "## Latest Notes".len()).min(content.len());
        let after = &content[after_start..];
        let section_end = after
            .find("\n## ")
            .map(|p| after_start + p)
            .unwrap_or(content.len());
        *content = format!(
            "{}\n## Latest Notes\n\n{}\n{}",
            content[..pos].trim_end(),
            note,
            &content[section_end..]
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command as StdCommand;
    use std::sync::OnceLock;

    use tempfile::TempDir;
    use tokio::sync::Mutex;

    use super::{execute, extract_must_fix_section, update_latest_notes};
    use crate::agent::router::AgentRouter;
    use crate::config::AppConfig;
    use crate::core::agent_workspace;
    use crate::core::cycle::{CycleDecision, Phase};
    use crate::core::profile::{ProfileName, ProfileRules};
    use crate::core::status::{Budget, Locks, ReviewStatus, StatusFile, StatusFrontmatter};
    use crate::core::task::{TaskState, TaskStore};
    use crate::machine_config::{LocalLlmCliConfig, ProviderMode};

    static TEST_WORKSPACE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn extract_must_fix_section_reads_only_must_fix_block() {
        let response = "Findings: High\nMust-fix:\n- tighten validation\n- add regression test\npaid_review_required: no\nreason: missing validation";
        let extracted = extract_must_fix_section(response);

        assert!(extracted.contains("Must-fix:"));
        assert!(extracted.contains("- tighten validation"));
        assert!(!extracted.contains("paid_review_required"));
    }

    #[test]
    fn extract_must_fix_section_returns_full_response_when_missing() {
        let response = "Findings: Low\n- (none)\npaid_review_required: no";
        assert_eq!(extract_must_fix_section(response), response);
    }

    #[test]
    fn update_latest_notes_replaces_existing_section() {
        let mut content = "## Goal\n\nShip it.\n\n## Latest Notes\n\nOld review\n\n## Next\n\nMore".to_string();
        update_latest_notes(&mut content, "New review");
        assert!(content.contains("New review"));
        assert!(!content.contains("Old review"));
    }

    #[tokio::test]
    async fn execute_files_open_task_when_review_finds_must_fix_items() {
        let _workspace_lock = test_workspace_lock().await;
        let temp = TempDir::new().expect("temp dir should be created");
        let _cwd = CurrentDirGuard::change_to(temp.path()).expect("cwd should switch");
        initialize_git_repo_with_review_target(temp.path());

        let orch_dir = temp.path().join(".orcha");
        let reviewer_dir = orch_dir.join("roles");
        std::fs::create_dir_all(&reviewer_dir).expect("reviewer role dir should exist");
        std::fs::write(reviewer_dir.join("reviewer.md"), "# reviewer\n").expect("reviewer role should be written");
        std::fs::create_dir_all(agent_workspace::dir(&orch_dir)).expect("agentworkspace should exist");
        std::fs::write(agent_workspace::status_log_path(&orch_dir), "# Status Log\n").expect("status log should be written");

        let script_path = write_fake_review_cli(temp.path()).expect("review cli script should be written");
        let mut config = minimal_cli_config(&script_path);
        config.local_llm_mode = ProviderMode::Cli;
        let router = AgentRouter::new(&config, &ProfileRules::from_name(ProfileName::LocalOnly), &std::collections::HashSet::new()).expect("router should build");
        let task_store = TaskStore::new(&orch_dir);
        task_store.ensure_dirs().await.expect("task dirs should exist");

        let mut status = StatusFile {
            frontmatter: StatusFrontmatter {
                run_id: "run-1".into(),
                profile: ProfileName::LocalOnly,
                cycle: 1,
                phase: Phase::Review,
                last_update: chrono::Utc::now().to_rfc3339(),
                budget: Budget { paid_calls_used: 0, paid_calls_limit: 2 },
                locks: Locks { writer: None, active_task: None },
                review_status: ReviewStatus::Clean,
                verify_status: None,
                consecutive_verify_failures: 0,
                disabled_agents: vec![],
            },
            content: "## Goal\n\nReview latest implementation.\n\n## Latest Notes\n\nPending review.\n".into(),
        };

        let decision = execute(&orch_dir, &mut status, &task_store, &router).await.expect("review should execute");
        assert_eq!(decision, CycleDecision::NextPhase);
        assert_eq!(status.frontmatter.review_status, ReviewStatus::Clean);
        assert!(status.content.contains("## Review (Cycle 1)"));

        let tasks = task_store.list_all().await.expect("tasks should load");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].state, TaskState::Open);
        assert_eq!(tasks[0].frontmatter.id, "R1");
        assert!(tasks[0].content.contains("tighten validation"));
    }

    async fn test_workspace_lock() -> tokio::sync::MutexGuard<'static, ()> {
        TEST_WORKSPACE_MUTEX.get_or_init(|| Mutex::new(())).lock().await
    }

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn change_to(path: &Path) -> anyhow::Result<Self> {
            let original = std::env::current_dir()?;
            std::env::set_current_dir(path)?;
            Ok(Self { original })
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    fn initialize_git_repo_with_review_target(workspace_root: &Path) {
        let tracked = workspace_root.join("src").join("work.txt");
        std::fs::create_dir_all(tracked.parent().expect("tracked file should have parent")).expect("src dir should exist");
        std::fs::write(&tracked, "baseline\nreview target\n").expect("review target should be written");

        run_git(workspace_root, &["init"]);
        run_git(workspace_root, &["config", "user.email", "test@example.com"]);
        run_git(workspace_root, &["config", "user.name", "orcha-test"]);
        run_git(workspace_root, &["add", "src/work.txt"]);
        run_git(workspace_root, &["commit", "-m", "baseline"]);
    }

    fn write_fake_review_cli(workspace_root: &Path) -> anyhow::Result<PathBuf> {
        let script_path = workspace_root.join("fake-review.ps1");
        let script = "[Console]::In.ReadToEnd() | Out-Null\nWrite-Output 'Findings: High'\nWrite-Output 'Must-fix:'\nWrite-Output '- tighten validation'\nWrite-Output '- add regression test'\nWrite-Output 'paid_review_required: no'\nWrite-Output 'reason: missing validation'\n";
        std::fs::write(&script_path, script)?;
        Ok(script_path)
    }

    fn minimal_cli_config(script_path: &Path) -> AppConfig {
        AppConfig {
            local_llm_mode: ProviderMode::Cli,
            local_llm_endpoint: String::new(),
            local_llm_model: "fake-reviewer".into(),
            local_llm_cli: LocalLlmCliConfig {
                command: powershell_command().to_string(),
                args: vec!["-NoProfile".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-File".into(), script_path.display().to_string()],
                prompt_via_stdin: true,
                model_arg: None,
                ensure_no_permission_flags: false,
                timeout_seconds: 60,
            },
            anthropic_api_key: None,
            anthropic_model: String::new(),
            anthropic_mode: ProviderMode::Http,
            anthropic_cli: LocalLlmCliConfig::default(),
            gemini_api_key: None,
            gemini_model: String::new(),
            gemini_mode: ProviderMode::Http,
            gemini_cli: LocalLlmCliConfig::default(),
            openai_api_key: None,
            codex_model: String::new(),
            openai_mode: ProviderMode::Http,
            openai_cli: LocalLlmCliConfig::default(),
        }
    }

    fn powershell_command() -> &'static str {
        #[cfg(windows)]
        {
            "powershell"
        }
        #[cfg(not(windows))]
        {
            "pwsh"
        }
    }

    fn run_git(workspace_root: &Path, args: &[&str]) {
        let status = StdCommand::new("git").args(args).current_dir(workspace_root).status().expect("git should run");
        assert!(status.success(), "git {:?} should succeed", args);
    }
}
