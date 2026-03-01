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
        s.to_string()
    } else {
        format!("{}... (truncated)", &s[..max_len])
    }
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
