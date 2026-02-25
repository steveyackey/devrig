use crate::config::model::{DevrigConfig, Port, ServiceConfig};
use std::collections::{BTreeMap, HashSet};
use std::net::TcpListener;

#[derive(Debug)]
pub struct PortConflict {
    pub service: String,
    pub port: u16,
    pub owner: Option<String>,
}

impl std::fmt::Display for PortConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.owner {
            Some(owner) => write!(
                f,
                "Port {} required by '{}' is already in use by {}",
                self.port, self.service, owner
            ),
            None => write!(
                f,
                "Port {} required by '{}' is already in use",
                self.port, self.service
            ),
        }
    }
}

pub fn check_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn find_free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("failed to bind ephemeral port")
        .local_addr()
        .unwrap()
        .port()
}

/// Find a free port that is not already in the allocated set.
pub fn find_free_port_excluding(allocated: &HashSet<u16>) -> u16 {
    for _ in 0..100 {
        let port = find_free_port();
        if !allocated.contains(&port) {
            return port;
        }
    }
    panic!("failed to find a free port after 100 attempts");
}

/// Resolve a single port from its config, respecting sticky auto-ports from
/// previous state.
pub fn resolve_port(
    resource_key: &str,
    port_config: &Port,
    prev_port: Option<u16>,
    prev_auto: bool,
    allocated: &mut HashSet<u16>,
) -> u16 {
    match port_config {
        Port::Fixed(p) => {
            allocated.insert(*p);
            *p
        }
        Port::Auto => {
            // Try to reuse previously assigned auto port
            if prev_auto {
                if let Some(prev) = prev_port {
                    if !allocated.contains(&prev) && check_port_available(prev) {
                        allocated.insert(prev);
                        return prev;
                    }
                    tracing::debug!(
                        "{}: previously assigned port {} no longer available",
                        resource_key,
                        prev
                    );
                }
            }
            let port = find_free_port_excluding(allocated);
            allocated.insert(port);
            port
        }
    }
}

/// Identify which process owns a given port.
pub fn identify_port_owner(port: u16) -> Option<String> {
    crate::platform::identify_port_owner(port)
}

/// Check all fixed ports (services + docker) for conflicts with already-bound
/// ports on the system.
pub fn check_all_ports_unified(config: &DevrigConfig) -> Vec<PortConflict> {
    let mut conflicts = Vec::new();

    for (name, svc) in &config.services {
        if let Some(Port::Fixed(port)) = &svc.port {
            if !check_port_available(*port) {
                conflicts.push(PortConflict {
                    service: name.clone(),
                    port: *port,
                    owner: identify_port_owner(*port),
                });
            }
        }
    }

    for (name, docker_cfg) in &config.docker {
        if let Some(Port::Fixed(port)) = &docker_cfg.port {
            if !check_port_available(*port) {
                conflicts.push(PortConflict {
                    service: format!("docker:{}", name),
                    port: *port,
                    owner: identify_port_owner(*port),
                });
            }
        }
        for (port_name, port_val) in &docker_cfg.ports {
            if let Port::Fixed(port) = port_val {
                if !check_port_available(*port) {
                    conflicts.push(PortConflict {
                        service: format!("docker:{}:{}", name, port_name),
                        port: *port,
                        owner: identify_port_owner(*port),
                    });
                }
            }
        }
    }

    // Check dashboard ports (only fixed ports — auto ports are resolved later)
    if let Some(dashboard) = &config.dashboard {
        if let Port::Fixed(dash_port) = &dashboard.port {
            if !check_port_available(*dash_port) {
                conflicts.push(PortConflict {
                    service: "dashboard".to_string(),
                    port: *dash_port,
                    owner: identify_port_owner(*dash_port),
                });
            }
        }

        let grpc = dashboard.otel.as_ref().map(|o| &o.grpc_port).cloned().unwrap_or(Port::Fixed(4317));
        if let Port::Fixed(grpc_port) = grpc {
            if !check_port_available(grpc_port) {
                conflicts.push(PortConflict {
                    service: "otel-grpc".to_string(),
                    port: grpc_port,
                    owner: identify_port_owner(grpc_port),
                });
            }
        }

        let http = dashboard.otel.as_ref().map(|o| &o.http_port).cloned().unwrap_or(Port::Fixed(4318));
        if let Port::Fixed(http_port) = http {
            if !check_port_available(http_port) {
                conflicts.push(PortConflict {
                    service: "otel-http".to_string(),
                    port: http_port,
                    owner: identify_port_owner(http_port),
                });
            }
        }
    }

    conflicts
}

/// Check all service ports only (backward compatibility).
pub fn check_all_ports(services: &BTreeMap<String, ServiceConfig>) -> Vec<PortConflict> {
    let mut conflicts = Vec::new();
    for (name, svc) in services {
        if let Some(Port::Fixed(port)) = &svc.port {
            if !check_port_available(*port) {
                conflicts.push(PortConflict {
                    service: name.clone(),
                    port: *port,
                    owner: identify_port_owner(*port),
                });
            }
        }
    }
    conflicts
}

pub fn format_port_conflicts(conflicts: &[PortConflict]) -> String {
    let mut msg = String::from("Port conflicts detected:\n");
    for conflict in conflicts {
        msg.push_str(&format!("  - {}\n", conflict));
    }
    msg.push_str("\nFree the ports or change your devrig.toml configuration.");
    msg
}
