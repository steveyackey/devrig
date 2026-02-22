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
                    tracing::info!(
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
#[cfg(target_os = "linux")]
pub fn identify_port_owner(port: u16) -> Option<String> {
    let tcp_content = std::fs::read_to_string("/proc/net/tcp").ok()?;
    let port_hex = format!("{:04X}", port);

    let mut target_inode: Option<String> = None;
    for line in tcp_content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let local_addr = fields[1];
        if let Some(addr_port) = local_addr.split(':').nth(1) {
            if addr_port == port_hex {
                target_inode = Some(fields[9].to_string());
                break;
            }
        }
    }

    let inode = target_inode?;
    if inode == "0" {
        return None;
    }

    let proc_dir = std::fs::read_dir("/proc").ok()?;
    for entry in proc_dir.flatten() {
        let pid_str = entry.file_name().to_string_lossy().to_string();
        if !pid_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let fd_dir = format!("/proc/{}/fd", pid_str);
        if let Ok(fds) = std::fs::read_dir(&fd_dir) {
            for fd_entry in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd_entry.path()) {
                    let link_str = link.to_string_lossy();
                    if link_str.contains(&format!("socket:[{}]", inode)) {
                        let cmdline_path = format!("/proc/{}/cmdline", pid_str);
                        if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                            let cmd = cmdline.replace('\0', " ").trim().to_string();
                            if cmd.is_empty() {
                                return Some(format!("PID {}", pid_str));
                            }
                            if cmd.len() > 60 {
                                return Some(format!("{}... (PID {})", &cmd[..57], pid_str));
                            }
                            return Some(format!("{} (PID {})", cmd, pid_str));
                        }
                        return Some(format!("PID {}", pid_str));
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
pub fn identify_port_owner(_port: u16) -> Option<String> {
    None
}

/// Check all fixed ports (services + infra) for conflicts with already-bound
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

    for (name, infra) in &config.infra {
        if let Some(Port::Fixed(port)) = &infra.port {
            if !check_port_available(*port) {
                conflicts.push(PortConflict {
                    service: format!("infra:{}", name),
                    port: *port,
                    owner: identify_port_owner(*port),
                });
            }
        }
        for (port_name, port_val) in &infra.ports {
            if let Port::Fixed(port) = port_val {
                if !check_port_available(*port) {
                    conflicts.push(PortConflict {
                        service: format!("infra:{}:{}", name, port_name),
                        port: *port,
                        owner: identify_port_owner(*port),
                    });
                }
            }
        }
    }

    // Check dashboard ports
    if let Some(dashboard) = &config.dashboard {
        let dash_port = dashboard.port;
        if !check_port_available(dash_port) {
            conflicts.push(PortConflict {
                service: "dashboard".to_string(),
                port: dash_port,
                owner: identify_port_owner(dash_port),
            });
        }

        let grpc_port = dashboard.otel.as_ref().map(|o| o.grpc_port).unwrap_or(4317);
        if !check_port_available(grpc_port) {
            conflicts.push(PortConflict {
                service: "otel-grpc".to_string(),
                port: grpc_port,
                owner: identify_port_owner(grpc_port),
            });
        }

        let http_port = dashboard.otel.as_ref().map(|o| o.http_port).unwrap_or(4318);
        if !check_port_available(http_port) {
            conflicts.push(PortConflict {
                service: "otel-http".to_string(),
                port: http_port,
                owner: identify_port_owner(http_port),
            });
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
