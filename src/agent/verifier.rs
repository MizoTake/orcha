use tokio::process::Command;

/// Result of running verification commands.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub passed: bool,
    pub command_results: Vec<CommandResult>,
    pub summary: String,
}

/// Result of a single command execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub passed: bool,
}

/// Run verification commands and return results.
pub async fn run(commands: &[String]) -> anyhow::Result<VerifyResult> {
    let mut results = Vec::new();
    let mut all_passed = true;

    for cmd in commands {
        let cmd = cmd.trim();
        if cmd.is_empty() || cmd.starts_with('#') {
            continue;
        }

        #[cfg(target_os = "windows")]
        let output = Command::new("cmd")
            .kill_on_drop(true)
            .args(["/C", cmd])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute '{}': {}", cmd, e))?;

        #[cfg(not(target_os = "windows"))]
        let output = Command::new("sh")
            .kill_on_drop(true)
            .arg("-c")
            .arg(cmd)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute '{}': {}", cmd, e))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let passed = output.status.success();

        if !passed {
            all_passed = false;
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        results.push(CommandResult {
            command: cmd.to_string(),
            exit_code,
            stdout: truncate(&stdout, 2000),
            stderr: truncate(&stderr, 2000),
            passed,
        });
    }

    let summary = if all_passed {
        format!("All {} commands passed", results.len())
    } else {
        let failed = results.iter().filter(|r| !r.passed).count();
        format!("{}/{} commands failed", failed, results.len())
    };

    Ok(VerifyResult {
        passed: all_passed,
        command_results: results,
        summary,
    })
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    // Find the largest char boundary that fits within max_len bytes.
    let boundary = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= max_len)
        .last()
        .unwrap_or(0);
    format!("{}... (truncated)", &s[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── truncate ─────────────────────────────────────────────────────────────

    #[test]
    fn truncate_returns_original_when_within_limit() {
        assert_eq!(truncate_for_test("hello", 10), "hello");
    }

    #[test]
    fn truncate_returns_original_at_exact_limit() {
        assert_eq!(truncate_for_test("hello", 5), "hello");
    }

    #[test]
    fn truncate_appends_suffix_when_over_limit() {
        let result = truncate_for_test("hello world", 5);
        assert_eq!(result, "hello... (truncated)");
    }

    #[test]
    fn truncate_empty_string_is_unchanged() {
        assert_eq!(truncate_for_test("", 10), "");
    }

    #[test]
    fn truncate_does_not_panic_on_multibyte_boundary() {
        // "テスト" is 9 bytes (3 bytes per char). Slicing at byte 5 would
        // land inside a character and previously caused a panic.
        let s = "テスト合格"; // 15 bytes
        let result = truncate_for_test(s, 5);
        // Must not panic; result must be valid UTF-8 ending at a char boundary.
        assert!(result.ends_with("... (truncated)"));
        let prefix = result.trim_end_matches("... (truncated)");
        assert!(s.starts_with(prefix));
    }

    // ── format_result ────────────────────────────────────────────────────────

    fn make_result(passed: bool) -> VerifyResult {
        VerifyResult {
            passed,
            summary: if passed {
                "All 1 commands passed".to_string()
            } else {
                "1/1 commands failed".to_string()
            },
            command_results: vec![CommandResult {
                command: "cargo test".to_string(),
                exit_code: if passed { 0 } else { 1 },
                stdout: "test output".to_string(),
                stderr: String::new(),
                passed,
            }],
        }
    }

    #[test]
    fn format_result_includes_command_and_status() {
        let result = make_result(true);
        let formatted = format_result(&result);
        assert!(formatted.contains("cargo test"));
        assert!(formatted.contains("PASS"));
        assert!(formatted.contains("Overall: PASS"));
    }

    #[test]
    fn format_result_shows_fail_for_failed_command() {
        let result = make_result(false);
        let formatted = format_result(&result);
        assert!(formatted.contains("FAIL"));
        assert!(formatted.contains("Overall: FAIL"));
    }

    #[test]
    fn format_result_includes_stdout_when_non_empty() {
        let result = make_result(true);
        let formatted = format_result(&result);
        assert!(formatted.contains("test output"));
    }

    #[test]
    fn format_result_omits_stdout_section_when_empty() {
        let mut result = make_result(true);
        result.command_results[0].stdout = String::new();
        let formatted = format_result(&result);
        assert!(!formatted.contains("Stdout:"));
    }

    // ── run ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn run_empty_command_list_returns_all_passed() {
        let result = run(&[]).await.expect("run should succeed");
        assert!(result.passed);
        assert!(result.command_results.is_empty());
        assert!(result.summary.contains("0"));
    }

    #[tokio::test]
    async fn run_skips_empty_and_comment_lines() {
        let commands = vec!["".to_string(), "# just a comment".to_string()];
        let result = run(&commands).await.expect("run should succeed");
        assert!(result.passed);
        assert!(result.command_results.is_empty());
    }

    #[tokio::test]
    async fn run_passing_command_reports_pass() {
        #[cfg(windows)]
        let cmd = "exit 0";
        #[cfg(not(windows))]
        let cmd = "true";

        let result = run(&[cmd.to_string()]).await.expect("run should succeed");
        assert!(result.passed);
        assert_eq!(result.command_results.len(), 1);
        assert!(result.command_results[0].passed);
        assert_eq!(result.command_results[0].exit_code, 0);
    }

    #[tokio::test]
    async fn run_failing_command_reports_fail() {
        #[cfg(windows)]
        let cmd = "exit 1";
        #[cfg(not(windows))]
        let cmd = "false";

        let result = run(&[cmd.to_string()]).await.expect("run should succeed");
        assert!(!result.passed);
        assert!(!result.command_results[0].passed);
        assert_ne!(result.command_results[0].exit_code, 0);
        assert!(result.summary.contains("1/1 commands failed"));
    }

    #[tokio::test]
    async fn run_truncates_long_stdout() {
        #[cfg(windows)]
        let cmd = format!("python -c \"print('x' * 3000)\"");
        #[cfg(not(windows))]
        let cmd = "printf '%3000s' | tr ' ' 'x'".to_string();

        let result = run(&[cmd]).await.expect("run should succeed");
        if let Some(r) = result.command_results.first() {
            assert!(r.stdout.len() <= 2000 + "... (truncated)".len());
        }
    }
}

#[cfg(test)]
pub(crate) fn truncate_for_test(s: &str, max_len: usize) -> String {
    truncate(s, max_len)
}

/// Format verify result as markdown for logging/display.
pub fn format_result(result: &VerifyResult) -> String {
    let mut out = String::new();
    for r in &result.command_results {
        out.push_str(&format!(
            "Command: {}\nExit code: {}\nStatus: {}\n",
            r.command,
            r.exit_code,
            if r.passed { "PASS" } else { "FAIL" }
        ));
        if !r.stdout.is_empty() {
            out.push_str(&format!("Stdout:\n```\n{}\n```\n", r.stdout.trim()));
        }
        if !r.stderr.is_empty() {
            out.push_str(&format!("Stderr:\n```\n{}\n```\n", r.stderr.trim()));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "Overall: {}\n",
        if result.passed { "PASS" } else { "FAIL" }
    ));
    out
}
