use clap::Parser;

use orcha::cli::{Cli, Command};
use orcha::config::AppConfig;
use orcha::core::error::OrchaError;

#[tokio::main]
async fn main() {
    // Load .env if present
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("orcha=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    if let Err(err) = run(cli).await {
        if let Some(orcha_err) = err.downcast_ref::<OrchaError>() {
            eprintln!("Error: {}", orcha_err);
            match orcha_err {
                OrchaError::NotInitialized { .. } => {
                    eprintln!("Hint: Run 'orcha init' to create the .orcha/ directory.");
                }
                OrchaError::AgentNotAvailable { agent, .. } => {
                    eprintln!("Hint: Set the API key environment variable for {}.", agent);
                }
                OrchaError::UnknownProfile { .. } => {
                    eprintln!(
                        "Available profiles: local_only, cheap_checkpoints, quality_gate, unblock_first"
                    );
                }
                _ => {}
            }
        } else {
            eprintln!("Error: {:?}", err);
        }
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let config = AppConfig::from_env();

    match cli.command {
        Command::Init => {
            orcha::cli::init::execute(&cli.orch_dir).await?;
        }
        Command::Run => {
            orcha::cli::run::execute(&cli.orch_dir, &config).await?;
        }
        Command::Status => {
            orcha::cli::status::execute(&cli.orch_dir).await?;
        }
        Command::Profile { name } => {
            orcha::cli::profile::execute(&cli.orch_dir, &name).await?;
        }
        Command::Explain => {
            orcha::cli::explain::execute(&cli.orch_dir, &config).await?;
        }
    }

    Ok(())
}
