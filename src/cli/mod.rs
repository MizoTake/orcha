pub mod explain;
pub mod init;
pub mod profile;
pub mod run;
pub mod status;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "orcha", about = "AI agent orchestration CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to .orcha directory
    #[arg(
        long = "orcha-dir",
        global = true,
        default_value = ".orcha"
    )]
    pub orch_dir: PathBuf,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .orcha/ directory
    Init,

    /// Execute cycles until goal is done or a stop condition is reached
    Run {
        /// Enforce single-writer lock (default: disabled for concurrent runs).
        #[arg(long, default_value_t = false)]
        enforce_lock: bool,

        /// Specification file path to bootstrap goal/tasks before starting.
        #[arg(long)]
        spec: Option<PathBuf>,
    },

    /// Display current status
    Status,

    /// Change active profile
    Profile {
        /// Profile name: built-ins (local_only, cheap_checkpoints, quality_gate, unblock_first, opencode_impl_no_review, opencode_impl_claude_review, opencode_impl_codex_review, claude_impl_opencode_review, codex_impl_opencode_review) or any name that has .orcha/profiles/<name>.md
        name: String,
    },

    /// Show current decision reasoning
    Explain,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn cli_uses_orcha_as_default_directory() {
        let cli = Cli::parse_from(["orcha", "status"]);
        assert!(matches!(cli.command, Command::Status));
        assert_eq!(cli.orch_dir, PathBuf::from(".orcha"));
        assert!(!cli.verbose);
    }

    #[test]
    fn cli_accepts_custom_orch_dir() {
        let cli = Cli::parse_from(["orcha", "--orcha-dir", ".orch", "run"]);
        assert!(matches!(
            cli.command,
            Command::Run {
                enforce_lock: false,
                spec: None,
            }
        ));
        assert_eq!(cli.orch_dir, PathBuf::from(".orch"));
    }

    #[test]
    fn cli_accepts_enforce_lock_flag_for_run() {
        let cli = Cli::parse_from(["orcha", "run", "--enforce-lock"]);
        assert!(matches!(
            cli.command,
            Command::Run {
                enforce_lock: true,
                spec: None,
            }
        ));
    }

    #[test]
    fn cli_accepts_spec_flag_for_run() {
        let cli = Cli::parse_from(["orcha", "run", "--spec", "requirements.md"]);
        assert!(matches!(
            cli.command,
            Command::Run {
                enforce_lock: false,
                spec: Some(_)
            }
        ));
    }

    #[test]
    fn cli_accepts_custom_orch_dir_after_subcommand() {
        let cli = Cli::parse_from(["orcha", "status", "--orcha-dir", ".orch"]);
        assert!(matches!(cli.command, Command::Status));
        assert_eq!(cli.orch_dir, PathBuf::from(".orch"));
    }
}
