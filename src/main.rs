use clap::{CommandFactory, Parser};
use clap_complete::aot::generate;
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
        Commands::Start {
            services,
            #[cfg(debug_assertions)]
            dev,
        } => {
            let dev_mode = { #[cfg(debug_assertions)] { dev } #[cfg(not(debug_assertions))] { false } };
            run_start(cli.global.config_file, services, dev_mode).await
        }
        Commands::Stop { .. } => run_stop(cli.global.config_file).await,
        Commands::Delete => run_delete(cli.global.config_file).await,
        Commands::Ps { all } => commands::ps::run(cli.global.config_file.as_deref(), all),
        Commands::Init => commands::init::run(),
        Commands::Doctor => commands::doctor::run(),
        Commands::Env { service } => {
            commands::env::run(cli.global.config_file.as_deref(), &service)
        }
        Commands::Exec { infra, command } => {
            commands::exec::run(cli.global.config_file.as_deref(), &infra, command).await
        }
        Commands::Reset { infra } => {
            commands::reset::run(cli.global.config_file.as_deref(), &infra)
        }
        Commands::Validate => commands::validate::run(cli.global.config_file.as_deref()),
        Commands::Logs {
            services,
            follow: _,
            tail,
            since,
            grep,
            exclude,
            level,
            format,
            output,
            timestamps,
        } => commands::logs::run(
            cli.global.config_file.as_deref(),
            services,
            tail,
            since,
            grep,
            exclude,
            level,
            format,
            output,
            timestamps,
        ),
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "devrig", &mut std::io::stdout());
            Ok(())
        }
        Commands::Cluster { command } => match command {
            devrig::cli::ClusterCommands::Create => {
                commands::cluster::run_create(cli.global.config_file.as_deref()).await
            }
            devrig::cli::ClusterCommands::Delete => {
                commands::cluster::run_delete(cli.global.config_file.as_deref()).await
            }
            devrig::cli::ClusterCommands::Kubeconfig => {
                commands::cluster::run_kubeconfig(cli.global.config_file.as_deref())
            }
        },
        Commands::Kubectl { args } => {
            commands::cluster::run_kubectl(cli.global.config_file.as_deref(), args).await
        }
        Commands::Skill { command } => match command {
            devrig::cli::SkillCommands::Install { global } => {
                commands::skill::run_install(global, cli.global.config_file.as_deref()).await
            }
        },
        Commands::Query { command } => match command {
            devrig::cli::QueryCommands::Traces {
                service,
                status,
                min_duration,
                last: _,
                limit,
                format,
            } => {
                commands::query::run_traces(
                    cli.global.config_file.as_deref(),
                    service,
                    status,
                    min_duration,
                    limit,
                    format,
                )
                .await
            }
            devrig::cli::QueryCommands::Trace { trace_id, format } => {
                commands::query::run_trace_detail(
                    cli.global.config_file.as_deref(),
                    trace_id,
                    format,
                )
                .await
            }
            devrig::cli::QueryCommands::Logs {
                service,
                level,
                search,
                trace_id,
                last: _,
                limit,
                format,
            } => {
                commands::query::run_logs(
                    cli.global.config_file.as_deref(),
                    service,
                    level,
                    search,
                    trace_id,
                    limit,
                    format,
                )
                .await
            }
            devrig::cli::QueryCommands::Metrics {
                name,
                service,
                last: _,
                limit,
                format,
            } => {
                commands::query::run_metrics(
                    cli.global.config_file.as_deref(),
                    name,
                    service,
                    limit,
                    format,
                )
                .await
            }
            devrig::cli::QueryCommands::Status { format } => {
                commands::query::run_status(cli.global.config_file.as_deref(), format).await
            }
            devrig::cli::QueryCommands::Related { trace_id, format } => {
                commands::query::run_related(cli.global.config_file.as_deref(), trace_id, format)
                    .await
            }
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

async fn run_start(
    config_file: Option<std::path::PathBuf>,
    services: Vec<String>,
    dev_mode: bool,
) -> anyhow::Result<()> {
    let config_path = resolve_config(config_file.as_deref())?;
    let mut orchestrator = Orchestrator::from_config(config_path)?;
    orchestrator.start(services, dev_mode).await
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
