use clap::{Args, Parser, Subcommand};
use clap_complete::aot::Shell;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "devrig", version, about = "Local development orchestrator")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Args)]
pub struct GlobalOpts {
    /// Use a specific config file
    #[arg(short = 'f', long = "file", global = true)]
    pub config_file: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start all services
    Start {
        /// Specific services to start (start all if empty)
        services: Vec<String>,
    },
    /// Stop all services
    Stop {
        /// Specific services to stop (stop all if empty)
        services: Vec<String>,
    },
    /// Stop and remove all resources
    Delete,
    /// Show service status
    Ps {
        /// Show all running devrig instances
        #[arg(long)]
        all: bool,
    },
    /// Generate a starter devrig.toml
    Init,
    /// Check that dependencies are installed
    Doctor,
    /// Show resolved environment variables for a service
    Env {
        /// Service name to show env for
        service: String,
    },
    /// Execute a command in an infra container
    Exec {
        /// Infrastructure service name
        infra: String,
        /// Command to execute
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Reset init-completed flag for an infra service
    Reset {
        /// Infrastructure service name
        infra: String,
    },

    /// Validate the configuration file
    Validate,

    /// Show and filter service logs
    Logs {
        /// Services to show logs for (all if empty)
        services: Vec<String>,

        /// Follow log output (live tail)
        #[arg(short = 'F', long)]
        follow: bool,

        /// Show last N lines
        #[arg(long)]
        tail: Option<usize>,

        /// Show logs since duration (e.g. "5m", "1h")
        #[arg(long)]
        since: Option<String>,

        /// Include only lines matching regex
        #[arg(short = 'g', long)]
        grep: Option<String>,

        /// Exclude lines matching regex
        #[arg(short = 'v', long)]
        exclude: Option<String>,

        /// Minimum log level (trace, debug, info, warn, error)
        #[arg(short = 'l', long)]
        level: Option<String>,

        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,

        /// Write output to file
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Show timestamps
        #[arg(short = 't', long)]
        timestamps: bool,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Manage the k3d cluster
    Cluster {
        #[command(subcommand)]
        command: ClusterCommands,
    },

    /// Proxy to kubectl with devrig's isolated kubeconfig
    #[command(name = "kubectl", alias = "k")]
    Kubectl {
        /// Arguments passed to kubectl
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Query telemetry data from the OTel collector
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum QueryCommands {
    /// List traces
    Traces {
        /// Filter by service name
        #[arg(short, long)]
        service: Option<String>,

        /// Filter by status (ok, error)
        #[arg(long)]
        status: Option<String>,

        /// Minimum duration in milliseconds
        #[arg(long)]
        min_duration: Option<u64>,

        /// Show traces from the last duration (e.g. "5m", "1h")
        #[arg(long)]
        last: Option<String>,

        /// Max results to return
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Output format: table, json, jsonl
        #[arg(long, alias = "output")]
        format: Option<String>,
    },

    /// Get details for a specific trace
    Trace {
        /// Trace ID (full or prefix)
        trace_id: String,

        /// Output format: table, json, jsonl
        #[arg(long, alias = "output")]
        format: Option<String>,
    },

    /// Query logs from the OTel collector
    Logs {
        /// Filter by service name
        #[arg(short, long)]
        service: Option<String>,

        /// Minimum severity (trace, debug, info, warn, error, fatal)
        #[arg(short = 'l', long, alias = "severity")]
        level: Option<String>,

        /// Search text in log body
        #[arg(short = 'g', long)]
        search: Option<String>,

        /// Filter by trace ID
        #[arg(long)]
        trace_id: Option<String>,

        /// Show logs from the last duration (e.g. "5m", "1h")
        #[arg(long)]
        last: Option<String>,

        /// Max results to return
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,

        /// Output format: table, json, jsonl
        #[arg(long, alias = "output")]
        format: Option<String>,
    },

    /// Query metrics from the OTel collector
    Metrics {
        /// Filter by metric name
        #[arg(short = 'm', long)]
        name: Option<String>,

        /// Filter by service name
        #[arg(short, long)]
        service: Option<String>,

        /// Show metrics from the last duration (e.g. "5m", "1h")
        #[arg(long)]
        last: Option<String>,

        /// Max results to return
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,

        /// Output format: table, json, jsonl
        #[arg(long, alias = "output")]
        format: Option<String>,
    },

    /// Show OTel collector status
    Status {
        /// Output format: table, json
        #[arg(long, alias = "output")]
        format: Option<String>,
    },

    /// Get related telemetry for a trace (logs and metrics from the same services)
    Related {
        /// Trace ID to find related telemetry for
        trace_id: String,

        /// Output format: table, json, jsonl
        #[arg(long, alias = "output")]
        format: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
    /// Create the k3d cluster
    Create,
    /// Delete the k3d cluster
    Delete,
    /// Print path to devrig's isolated kubeconfig
    Kubeconfig,
}
