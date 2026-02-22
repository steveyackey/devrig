use clap::{Args, Parser, Subcommand};
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
