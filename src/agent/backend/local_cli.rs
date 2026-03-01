use std::path::Path;
use std::process::Stdio;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;
use crate::machine_config::LocalLlmCliConfig;

/// Local CLI agent that directly invokes an external command.
pub struct LocalCliAgent {
    command: String,
    args: Vec<String>,
    model: String,
    model_arg: Option<String>,
    prompt_via_stdin: bool,
    ensure_no_permission_flags: bool,
    agent_kind: AgentKind,
}

impl LocalCliAgent {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        Self::from_cli_config(&config.local_llm_cli, &config.local_llm_model, AgentKind::LocalLlm)
    }

    pub fn from_cli_config(cli: &LocalLlmCliConfig, model: &str, kind: AgentKind) -> anyhow::Result<Self> {
        if cli.command.trim().is_empty() {
            anyhow::bail!(
                "CLI mode requires command in CLI config"
            );
        }

        Ok(Self {
            command: cli.command.clone(),
            args: with_no_permission_flags(&cli.command, &cli.args, cli.ensure_no_permission_flags),
            model: model.to_string(),
            model_arg: cli.model_arg.clone(),
            prompt_via_stdin: cli.prompt_via_stdin,
            ensure_no_permission_flags: cli.ensure_no_permission_flags,
            agent_kind: kind,
        })
    }

    fn build_prompt(&self, context: &AgentContext) -> String {
        let mut prompt = String::new();
        for file in &context.context_files {
            prompt.push_str(&format!("--- {} ---\n{}\n\n", file.name, file.content));
        }
        prompt.push_str("--- Instruction ---\n");
        prompt.push_str(&context.instruction);
        prompt
    }
}

fn with_no_permission_flags(command: &str, args: &[String], enabled: bool) -> Vec<String> {
    if !enabled {
        return args.to_vec();
    }

    let command_name = normalize_command_name(command);
    let mut resolved = args.to_vec();

    if is_codex_command(&command_name) {
        ensure_codex_no_permission_args(&mut resolved);
    } else if is_claude_code_command(&command_name) {
        ensure_claude_no_permission_args(&mut resolved);
    }

    resolved
}

fn normalize_command_name(command: &str) -> String {
    let raw = Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase();

    for suffix in [".exe", ".cmd", ".bat"] {
        if let Some(stripped) = raw.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    raw
}

fn is_codex_command(command_name: &str) -> bool {
    command_name == "codex"
}

fn is_claude_code_command(command_name: &str) -> bool {
    command_name == "claude" || command_name == "claude-code" || command_name == "claudecode"
}

fn is_opencode_command(command_name: &str) -> bool {
    command_name == "opencode" || command_name == "opencode-cli"
}

const OPENCODE_PERMISSION_ALLOW_ALL: &str = r#"{"*":"allow","doom_loop":"allow"}"#;

fn opencode_permission_env_value(command_name: &str, enabled: bool) -> Option<&'static str> {
    if enabled && is_opencode_command(command_name) {
        Some(OPENCODE_PERMISSION_ALLOW_ALL)
    } else {
        None
    }
}

fn ensure_codex_no_permission_args(args: &mut Vec<String>) {
    let has_dangerous = args.iter().any(|arg| {
        arg.eq_ignore_ascii_case("--dangerously-bypass-approvals-and-sandbox")
            || arg.eq_ignore_ascii_case("--yolo")
    });
    if has_dangerous {
        return;
    }

    let has_never =
        has_flag_value(args, "--ask-for-approval", "never") || has_flag_value(args, "-a", "never");
    if !has_never {
        args.push("--ask-for-approval".to_string());
        args.push("never".to_string());
    }
}

fn ensure_claude_no_permission_args(args: &mut Vec<String>) {
    let has_skip = args
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("--dangerously-skip-permissions"));
    if !has_skip {
        args.push("--dangerously-skip-permissions".to_string());
    }
}

fn has_flag_value(args: &[String], flag: &str, value: &str) -> bool {
    args.iter().enumerate().any(|(idx, arg)| {
        if arg.eq_ignore_ascii_case(flag) {
            return args
                .get(idx + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case(value));
        }

        let inline = format!("{flag}={value}");
        arg.eq_ignore_ascii_case(&inline)
    })
}

fn is_false_like(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

fn has_enabled_thinking_flag(args: &[String]) -> bool {
    for (idx, arg) in args.iter().enumerate() {
        if arg.eq_ignore_ascii_case("--thinking") {
            if let Some(next) = args.get(idx + 1) {
                if is_false_like(next) {
                    continue;
                }
            }
            return true;
        }

        if let Some((flag, value)) = arg.split_once('=') {
            if flag.eq_ignore_ascii_case("--thinking") {
                return !is_false_like(value);
            }
        }
    }

    false
}

#[async_trait]
impl Agent for LocalCliAgent {
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse> {
        let prompt = self.build_prompt(context);
        let request_preview = summarize_request(&context.instruction, 120);
        let command_name = normalize_command_name(&self.command);
        let thinking_enabled =
            is_opencode_command(&command_name) && has_enabled_thinking_flag(&self.args);

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        if let Some(permission) =
            opencode_permission_env_value(&command_name, self.ensure_no_permission_flags)
        {
            cmd.env("OPENCODE_PERMISSION", permission);
        }
        if let Some(model_arg) = &self.model_arg {
            if self.model.trim().is_empty() {
                // When model is omitted in orcha.yml, let the CLI use its own default model.
                // Do not append model flag/value in this case.
            } else {
                cmd.arg(model_arg);
                cmd.arg(&self.model);
            }
        }

        if self.prompt_via_stdin {
            cmd.stdin(Stdio::piped());
        } else {
            // opencode run uses positional message parsing; prompts beginning with '-' can be
            // misread as options unless we terminate option parsing explicitly.
            if is_opencode_command(&command_name) {
                cmd.arg("--");
            }
            cmd.arg(&prompt);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start CLI '{}': {}", self.command, e))?;
        let child_pid = child.id();

        if self.prompt_via_stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write prompt to CLI stdin: {}", e))?;
            }
        }

        let output = wait_with_output_and_heartbeat(
            child,
            &self.command,
            child_pid,
            self.prompt_via_stdin,
            &self.model,
            &request_preview,
            thinking_enabled,
        )
        .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else {
                stdout
            };
            anyhow::bail!(
                "Local CLI '{}' failed with exit code {:?}: {}",
                self.command,
                output.status.code(),
                detail
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            println!(
                "  ... local CLI stderr: {} chars preview=\"{}\"",
                stderr.chars().count(),
                summarize_request(&stderr, 160)
            );
        }
        if !stdout.is_empty() {
            println!(
                "  ... local CLI response: {} chars preview=\"{}\"",
                stdout.chars().count(),
                summarize_request(&stdout, 160)
            );
        }

        if stdout.is_empty() {
            anyhow::bail!("Local CLI '{}' returned empty stdout", self.command);
        }

        Ok(AgentResponse {
            content: stdout,
            model_used: format!("cli:{}:{}", self.command, self.model),
            tokens_used: None,
            is_paid: false,
        })
    }

    fn kind(&self) -> AgentKind {
        self.agent_kind
    }
}

async fn wait_with_output_and_heartbeat(
    mut child: tokio::process::Child,
    command: &str,
    child_pid: Option<u32>,
    prompt_via_stdin: bool,
    model: &str,
    request_preview: &str,
    thinking_enabled: bool,
) -> anyhow::Result<std::process::Output> {
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
    let pid = child_pid
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let prompt_mode = if prompt_via_stdin { "stdin" } else { "arg" };
    let model_name = if model.trim().is_empty() {
        "(default)"
    } else {
        model
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture CLI stdout for '{}'", command))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture CLI stderr for '{}'", command))?;

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let stdout_task = tokio::spawn(read_stream_lines(stdout, CliStreamKind::Stdout, event_tx.clone()));
    let stderr_task = tokio::spawn(read_stream_lines(stderr, CliStreamKind::Stderr, event_tx.clone()));
    drop(event_tx);

    print_inline_status(&format!(
        "  ... local CLI start: command='{}' pid={} prompt_mode={} model={} request=\"{}\"",
        command, pid, prompt_mode, model_name, request_preview
    ));

    let started_at = Instant::now();
    let mut wait_future = Box::pin(child.wait());
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let mut status = None;
    let mut stream_open = true;
    let mut last_thinking = String::new();

    while status.is_none() || stream_open {
        tokio::select! {
            wait_result = &mut wait_future, if status.is_none() => {
                match wait_result {
                    Ok(wait_status) => {
                        status = Some(wait_status);
                    }
                    Err(e) => {
                        stdout_task.abort();
                        stderr_task.abort();
                        finish_inline_status(&format!(
                            "  ... local CLI wait failed: command='{}' pid={} elapsed={}s request=\"{}\"",
                            command,
                            pid,
                            started_at.elapsed().as_secs(),
                            request_preview
                        ));
                        return Err(anyhow::anyhow!("Failed waiting for CLI '{}': {}", command, e));
                    }
                }
            }
            event = event_rx.recv(), if stream_open => {
                match event {
                    Some(event) => {
                        if thinking_enabled && matches!(event.kind, CliStreamKind::Stdout) {
                            if let Some(update) = extract_thinking_update(&event.line) {
                                if update != last_thinking {
                                    last_thinking = update;
                                    print_inline_status(&format!(
                                        "  ... local CLI thinking: command='{}' pid={} elapsed={}s thinking=\"{}\"",
                                        command,
                                        pid,
                                        started_at.elapsed().as_secs(),
                                        last_thinking
                                    ));
                                }
                            }
                        }
                    }
                    None => {
                        stream_open = false;
                    }
                }
            }
            _ = heartbeat.tick(), if status.is_none() => {
                if last_thinking.is_empty() {
                    print_inline_status(&format!(
                        "  ... local CLI waiting: command='{}' pid={} elapsed={}s request=\"{}\"",
                        command,
                        pid,
                        started_at.elapsed().as_secs(),
                        request_preview
                    ));
                } else {
                    print_inline_status(&format!(
                        "  ... local CLI waiting: command='{}' pid={} elapsed={}s thinking=\"{}\"",
                        command,
                        pid,
                        started_at.elapsed().as_secs(),
                        last_thinking
                    ));
                }
            }
        }
    }

    let status = status.ok_or_else(|| anyhow::anyhow!("CLI '{}' exited without status", command))?;
    let stdout_text = stdout_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stdout reader for '{}': {}", command, e))?
        .map_err(|e| anyhow::anyhow!("Failed to read stdout for '{}': {}", command, e))?;
    let stderr_text = stderr_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stderr reader for '{}': {}", command, e))?
        .map_err(|e| anyhow::anyhow!("Failed to read stderr for '{}': {}", command, e))?;

    finish_inline_status(&format!(
        "  ... local CLI done: command='{}' pid={} elapsed={}s exit={:?} request=\"{}\"",
        command,
        pid,
        started_at.elapsed().as_secs(),
        status.code(),
        request_preview
    ));

    Ok(std::process::Output {
        status,
        stdout: stdout_text.into_bytes(),
        stderr: stderr_text.into_bytes(),
    })
}

#[derive(Clone, Copy)]
enum CliStreamKind {
    Stdout,
    Stderr,
}

struct CliStreamEvent {
    kind: CliStreamKind,
    line: String,
}

async fn read_stream_lines<R>(
    reader: R,
    kind: CliStreamKind,
    event_tx: mpsc::UnboundedSender<CliStreamEvent>,
) -> io::Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut collected = String::new();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if !collected.is_empty() {
            collected.push('\n');
        }
        collected.push_str(&line);
        let _ = event_tx.send(CliStreamEvent { kind, line });
    }
    Ok(collected)
}

fn extract_thinking_update(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return extract_thinking_from_json(&value).map(|v| summarize_request(&v, 160));
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("thinking") || lower.contains("reasoning") {
        return Some(summarize_request(trimmed, 160));
    }

    None
}

fn extract_thinking_from_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, current) in map {
                let key_lower = key.to_ascii_lowercase();
                if key_lower.contains("thinking") || key_lower.contains("reasoning") {
                    if let Some(text) = json_value_to_text(current) {
                        if !text.is_empty() {
                            return Some(text);
                        }
                    }
                }
            }

            for current in map.values() {
                if let Some(found) = extract_thinking_from_json(current) {
                    return Some(found);
                }
            }

            None
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(found) = extract_thinking_from_json(item) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn json_value_to_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Array(items) => {
            let joined = items
                .iter()
                .filter_map(json_value_to_text)
                .collect::<Vec<_>>()
                .join(" ");
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }
        serde_json::Value::Object(map) => map
            .get("text")
            .and_then(json_value_to_text)
            .or_else(|| map.get("content").and_then(json_value_to_text)),
        _ => None,
    }
}

fn summarize_request(raw: &str, max_chars: usize) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let truncated: String = compact.chars().take(max_chars).collect();
    format!("{truncated}...")
}

fn print_inline_status(message: &str) {
    print!("\r\x1b[2K{}", message);
    let _ = io::stdout().flush();
}

fn finish_inline_status(message: &str) {
    print_inline_status(message);
    println!();
}

#[cfg(test)]
mod tests {
    use crate::agent::{Agent, AgentContext, AgentKind, ContextFile};
    use crate::config::AppConfig;
    use crate::machine_config::{LocalLlmCliConfig, ProviderMode};

    use super::LocalCliAgent;

    fn sample_config(command: &str) -> AppConfig {
        sample_config_with(command, default_cli_args(), true)
    }

    fn sample_config_with(
        command: &str,
        args: Vec<String>,
        ensure_no_permission: bool,
    ) -> AppConfig {
        AppConfig {
            local_llm_mode: ProviderMode::Cli,
            local_llm_endpoint: "http://localhost:11434/v1".to_string(),
            local_llm_model: "llama3.2".to_string(),
            local_llm_cli: LocalLlmCliConfig {
                command: command.to_string(),
                args,
                prompt_via_stdin: true,
                model_arg: None,
                ensure_no_permission_flags: ensure_no_permission,
            },
            anthropic_api_key: None,
            anthropic_model: "claude-sonnet-4-20250514".to_string(),
            anthropic_mode: ProviderMode::Http,
            anthropic_cli: LocalLlmCliConfig::default(),
            gemini_api_key: None,
            gemini_model: "gemini-2.0-flash".to_string(),
            gemini_mode: ProviderMode::Http,
            gemini_cli: LocalLlmCliConfig::default(),
            openai_api_key: None,
            codex_model: "gpt-4.1".to_string(),
            openai_mode: ProviderMode::Http,
            openai_cli: LocalLlmCliConfig::default(),
        }
    }

    #[cfg(windows)]
    fn default_cli_args() -> Vec<String> {
        vec!["/C".to_string(), "more".to_string()]
    }

    #[cfg(not(windows))]
    fn default_cli_args() -> Vec<String> {
        vec![]
    }

    #[test]
    fn new_fails_when_command_is_empty() {
        let err = LocalCliAgent::from_cli_config(
            &LocalLlmCliConfig {
                command: "".to_string(),
                args: vec![],
                prompt_via_stdin: false,
                model_arg: None,
                ensure_no_permission_flags: true,
            },
            "llama3.2",
            AgentKind::LocalLlm,
        ).err().expect("expected error");
        assert!(err
            .to_string()
            .contains("requires command"));
    }

    #[test]
    fn new_succeeds_when_command_is_set() {
        let cfg = sample_config("opencode");
        let agent = LocalCliAgent::new(&cfg);
        assert!(agent.is_ok());
    }

    #[test]
    fn injects_codex_no_permission_args() {
        let cfg = sample_config_with("codex", vec!["exec".to_string()], true);
        let agent = LocalCliAgent::new(&cfg).expect("agent should build");
        assert_eq!(
            agent.args,
            vec![
                "exec".to_string(),
                "--ask-for-approval".to_string(),
                "never".to_string()
            ]
        );
    }

    #[test]
    fn does_not_duplicate_existing_codex_permission_args() {
        let cfg = sample_config_with(
            "codex",
            vec![
                "exec".to_string(),
                "--ask-for-approval".to_string(),
                "never".to_string(),
            ],
            true,
        );
        let agent = LocalCliAgent::new(&cfg).expect("agent should build");

        let count = agent
            .args
            .iter()
            .filter(|arg| arg.as_str() == "--ask-for-approval")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn injects_claude_skip_permissions_arg() {
        let cfg = sample_config_with("claude", vec!["-p".to_string()], true);
        let agent = LocalCliAgent::new(&cfg).expect("agent should build");
        assert!(agent
            .args
            .iter()
            .any(|arg| arg == "--dangerously-skip-permissions"));
    }

    #[test]
    fn can_disable_auto_permission_flags() {
        let cfg = sample_config_with("codex", vec!["exec".to_string()], false);
        let agent = LocalCliAgent::new(&cfg).expect("agent should build");
        assert!(!agent
            .args
            .iter()
            .any(|arg| arg == "--ask-for-approval" || arg == "never"));
    }

    #[test]
    fn does_not_append_model_when_model_is_empty() {
        let mut cfg = sample_config_with("opencode", vec!["chat".to_string()], true);
        cfg.local_llm_model = String::new();
        cfg.local_llm_cli.model_arg = Some("--model".to_string());

        let agent = LocalCliAgent::new(&cfg).expect("agent should build");
        assert!(agent.model.is_empty());
        assert_eq!(agent.model_arg.as_deref(), Some("--model"));
    }

    #[test]
    fn recognizes_opencode_command_name_variants() {
        assert!(super::is_opencode_command("opencode"));
        assert!(super::is_opencode_command("opencode-cli"));
        assert!(!super::is_opencode_command("codex"));
    }

    #[test]
    fn opencode_permission_env_is_enabled_only_for_opencode_command() {
        assert_eq!(
            super::opencode_permission_env_value("opencode", true),
            Some(super::OPENCODE_PERMISSION_ALLOW_ALL)
        );
        assert_eq!(
            super::opencode_permission_env_value("opencode-cli", true),
            Some(super::OPENCODE_PERMISSION_ALLOW_ALL)
        );
        assert_eq!(super::opencode_permission_env_value("opencode", false), None);
        assert_eq!(super::opencode_permission_env_value("codex", true), None);
    }

    #[test]
    fn detects_enabled_thinking_flag_variants() {
        assert!(super::has_enabled_thinking_flag(&["--thinking".to_string()]));
        assert!(super::has_enabled_thinking_flag(&["--thinking=true".to_string()]));
        assert!(super::has_enabled_thinking_flag(&["--thinking=yes".to_string()]));
        assert!(!super::has_enabled_thinking_flag(&["--thinking=false".to_string()]));
        assert!(!super::has_enabled_thinking_flag(&[
            "--thinking".to_string(),
            "false".to_string()
        ]));
        assert!(!super::has_enabled_thinking_flag(&[]));
    }

    #[test]
    fn extracts_thinking_update_from_json_line() {
        let line = r#"{"type":"delta","thinking":"inspect files first"}"#;
        assert_eq!(
            super::extract_thinking_update(line).as_deref(),
            Some("inspect files first")
        );
    }

    #[test]
    fn extracts_thinking_update_from_plain_text_line() {
        let line = "thinking: gather current status";
        assert_eq!(
            super::extract_thinking_update(line).as_deref(),
            Some("thinking: gather current status")
        );
    }

    #[test]
    fn ignores_non_thinking_output_line() {
        let line = "implementation completed";
        assert_eq!(super::extract_thinking_update(line), None);
    }

    #[cfg(windows)]
    fn echo_stdin_command() -> &'static str {
        "cmd"
    }

    #[cfg(not(windows))]
    fn echo_stdin_command() -> &'static str {
        "cat"
    }

    #[tokio::test]
    async fn respond_returns_stdout_from_cli() {
        let cfg = sample_config(echo_stdin_command());
        let agent = LocalCliAgent::new(&cfg).expect("agent should build");

        let context = AgentContext {
            context_files: vec![ContextFile {
                name: "sample.md".to_string(),
                content: "context body".to_string(),
            }],
            instruction: "say hello".to_string(),
        };

        let response = agent.respond(&context).await.expect("CLI should respond");
        assert!(response.content.contains("sample.md"));
        assert!(response.content.contains("say hello"));
    }
}
