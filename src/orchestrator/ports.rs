use crate::config::model::{Port, ServiceConfig};
use std::collections::BTreeMap;
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

/// Identify which process owns a given port.
/// On Linux, parses /proc/net/tcp and /proc/*/fd to find the owning process.
/// Returns None on non-Linux or if identification fails.
#[cfg(target_os = "linux")]
pub fn identify_port_owner(port: u16) -> Option<String> {
    // Parse /proc/net/tcp to find the inode for this port
    let tcp_content = std::fs::read_to_string("/proc/net/tcp").ok()?;
    let port_hex = format!("{:04X}", port);

    let mut target_inode: Option<String> = None;
    for line in tcp_content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        // local_address is field[1], format is "IP:PORT" in hex
        let local_addr = fields[1];
        if let Some(addr_port) = local_addr.split(':').nth(1) {
            if addr_port == port_hex || addr_port == format!("{:04X}", port) {
                target_inode = Some(fields[9].to_string());
                break;
            }
        }
    }

    let inode = target_inode?;
    if inode == "0" {
        return None;
    }

    // Scan /proc/*/fd/ to find which PID owns this inode
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
                        // Found the PID, read cmdline
                        let cmdline_path = format!("/proc/{}/cmdline", pid_str);
                        if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                            let cmd = cmdline.replace('\0', " ").trim().to_string();
                            if cmd.is_empty() {
                                return Some(format!("PID {}", pid_str));
                            }
                            // Truncate long commands
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
