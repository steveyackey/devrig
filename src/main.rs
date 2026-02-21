use clap::Parser;
use devrig::cli::{Cli, Commands};
use devrig::commands;
use devrig::config::resolve::resolve_config;
use devrig::orchestrator::Orchestrator;

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber with env-filter support.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Start { services } => run_start(cli.global.config_file, services).await,
        Commands::Stop { .. } => run_stop(cli.global.config_file).await,
        Commands::Delete => run_delete(cli.global.config_file).await,
        Commands::Ps { all } => commands::ps::run(cli.global.config_file.as_deref(), all),
        Commands::Init => commands::init::run(),
        Commands::Doctor => commands::doctor::run(),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

async fn run_start(
    config_file: Option<std::path::PathBuf>,
    services: Vec<String>,
) -> anyhow::Result<()> {
    let config_path = resolve_config(config_file.as_deref())?;
    let orchestrator = Orchestrator::from_config(config_path)?;
    orchestrator.start(services).await
}

async fn run_stop(config_file: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    let config_path = resolve_config(config_file.as_deref())?;
    let orchestrator = Orchestrator::from_config(config_path)?;
    orchestrator.stop().await
}

async fn run_delete(config_file: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    let config_path = resolve_config(config_file.as_deref())?;
    let orchestrator = Orchestrator::from_config(config_path)?;
    orchestrator.delete().await
}
