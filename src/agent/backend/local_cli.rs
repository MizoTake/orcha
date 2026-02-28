use std::path::Path;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::agent::{Agent, AgentContext, AgentKind, AgentResponse};
use crate::config::AppConfig;

/// Local CLI agent that directly invokes an external command.
pub struct LocalCliAgent {
    command: String,
    args: Vec<String>,
    model: String,
    model_arg: Option<String>,
    prompt_via_stdin: bool,
}

impl LocalCliAgent {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let cli = &config.local_llm_cli;
        if cli.command.trim().is_empty() {
            anyhow::bail!(
                "agents.local_llm.mode=cli requires agents.local_llm.cli.command in orcha.yml"
            );
        }

        Ok(Self {
            command: cli.command.clone(),
            args: with_no_permission_flags(&cli.command, &cli.args, cli.ensure_no_permission_flags),
            model: config.local_llm_model.clone(),
            model_arg: cli.model_arg.clone(),
            prompt_via_stdin: cli.prompt_via_stdin,
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

#[async_trait]
impl Agent for LocalCliAgent {
    async fn respond(&self, context: &AgentContext) -> anyhow::Result<AgentResponse> {
        let prompt = self.build_prompt(context);

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        if let Some(model_arg) = &self.model_arg {
            cmd.arg(model_arg);
            cmd.arg(&self.model);
        }

        if self.prompt_via_stdin {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.arg(&prompt);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start CLI '{}': {}", self.command, e))?;

        if self.prompt_via_stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write prompt to CLI stdin: {}", e))?;
            }
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed waiting for CLI '{}': {}", self.command, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!(
                "Local CLI '{}' failed with exit code {:?}: {}",
                self.command,
                output.status.code(),
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
        AgentKind::LocalLlm
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::{Agent, AgentContext, ContextFile};
    use crate::config::AppConfig;
    use crate::machine_config::{LocalLlmCliConfig, LocalLlmMode};

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
            local_llm_mode: LocalLlmMode::Cli,
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
            gemini_api_key: None,
            gemini_model: "gemini-2.0-flash".to_string(),
            openai_api_key: None,
            codex_model: "gpt-4.1".to_string(),
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
        let cfg = sample_config("");
        let err = LocalCliAgent::new(&cfg).err().expect("expected error");
        assert!(err
            .to_string()
            .contains("requires agents.local_llm.cli.command"));
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
