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

    /// Path to .orch directory
    #[arg(long, default_value = ".orch")]
    pub orch_dir: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .orcha/ directory with templates
    Init,

    /// Execute one step from current phase
    Run,

    /// Display current status
    Status,

    /// Change active profile
    Profile {
        /// Profile name: local_only, cheap_checkpoints, quality_gate, unblock_first
        name: String,
    },

    /// Show current decision reasoning
    Explain,
}
