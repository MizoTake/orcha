use clap::Parser;

use orcha::cli::{Cli, Command};
use orcha::core::error::OrchaError;
use orcha::core::profile::ProfileName;

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

    if let Err(err) = run_with_interrupt(cli).await {
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
                    let available = ProfileName::all()
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!(
                        "Available profiles: {}",
                        available
                    );
                }
                OrchaError::MachineConfigError { .. } => {
                    eprintln!("Hint: Ensure .orcha/orcha.yml exists and is valid YAML.");
                }
                _ => {}
            }
        } else {
            eprintln!("Error: {:?}", err);
        }
        std::process::exit(1);
    }
}

async fn run_with_interrupt(cli: Cli) -> anyhow::Result<()> {
    let orch_dir = cli.orch_dir.clone();
    if !matches!(&cli.command, Command::Run { .. }) {
        return run(cli).await;
    }

    tokio::select! {
        result = run(cli) => result,
        signal_result = tokio::signal::ctrl_c() => {
            signal_result.map_err(|e| anyhow::anyhow!("Failed to listen for Ctrl+C: {}", e))?;

            if let Err(err) = orcha::cli::run::release_writer_lock_for_current_process(&orch_dir).await {
                tracing::warn!("Failed to release writer lock after Ctrl+C: {}", err);
            }

            Err(OrchaError::StopCondition {
                reason: "Interrupted by Ctrl+C".to_string(),
            }.into())
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Init => {
            orcha::cli::init::execute(&cli.orch_dir).await?;
        }
        Command::Run {
            enforce_lock,
            spec,
            reset_cycle,
            no_timeout,
        } => {
            let config = orcha::config::AppConfig::from_orch_dir(&cli.orch_dir)?;
            orcha::cli::run::execute(
                &cli.orch_dir,
                &config,
                !enforce_lock,
                spec.as_deref(),
                reset_cycle,
                no_timeout,
            )
            .await?;
        }
        Command::Status => {
            orcha::cli::status::execute(&cli.orch_dir).await?;
        }
        Command::Profile { name } => {
            orcha::cli::profile::execute(&cli.orch_dir, &name).await?;
        }
        Command::Explain => {
            let config = orcha::config::AppConfig::from_orch_dir(&cli.orch_dir)?;
            orcha::cli::explain::execute(&cli.orch_dir, &config).await?;
        }
    }

    Ok(())
}
