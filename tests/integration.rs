#![cfg(feature = "integration")]

mod common;

#[path = "integration/cluster_lifecycle.rs"]
mod cluster_lifecycle;
#[path = "integration/cluster_network.rs"]
mod cluster_network;
#[path = "integration/cluster_registry.rs"]
mod cluster_registry;
#[path = "integration/completions.rs"]
mod completions;
#[path = "integration/compose_interop.rs"]
mod compose_interop;
#[path = "integration/config_diff.rs"]
mod config_diff;
#[path = "integration/config_file_flag.rs"]
mod config_file_flag;
#[path = "integration/crash_recovery.rs"]
mod crash_recovery;
#[path = "integration/dashboard_api.rs"]
mod dashboard_api;
#[path = "integration/dir_discovery.rs"]
mod dir_discovery;
#[path = "integration/env_command.rs"]
mod env_command;
#[path = "integration/infra_lifecycle.rs"]
mod infra_lifecycle;
#[path = "integration/init_scripts.rs"]
mod init_scripts;
#[path = "integration/label_cleanup.rs"]
mod label_cleanup;
#[path = "integration/leaked_resources.rs"]
mod leaked_resources;
#[path = "integration/multi_instance.rs"]
mod multi_instance;
#[path = "integration/network_tests.rs"]
mod network_tests;
#[path = "integration/otel_ingest.rs"]
mod otel_ingest;
#[path = "integration/port_collision.rs"]
mod port_collision;
#[path = "integration/ps_all.rs"]
mod ps_all;
#[path = "integration/ready_checks.rs"]
mod ready_checks;
#[path = "integration/reset_command.rs"]
mod reset_command;
#[path = "integration/service_discovery.rs"]
mod service_discovery;
#[path = "integration/start_stop.rs"]
mod start_stop;
#[path = "integration/validate_command.rs"]
mod validate_command;
#[path = "integration/volume_cleanup.rs"]
mod volume_cleanup;
