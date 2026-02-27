use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;

/// No-op handle on Unix — process group cleanup uses killpg with the child PID.
pub struct ProcessGroupHandle;

/// Return the user's default shell from `$SHELL`, falling back to `sh`.
fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

/// Human-readable description of the shell for log messages.
pub fn shell_name() -> String {
    let shell = user_shell();
    format!("{} -lc", shell)
}

pub fn shell_command(command: &str) -> Command {
    let shell = user_shell();
    let mut cmd = Command::new(&shell);
    // Login shell (-l) sources the user's profile/rc files so that
    // PATH and other environment customisations are available.
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn configure_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

pub fn post_spawn_setup(_child_pid: Option<u32>) -> Option<ProcessGroupHandle> {
    // On Unix, process group is configured before spawn via process_group(0).
    None
}

pub async fn terminate_child(
    child: &mut tokio::process::Child,
    child_pid: Option<u32>,
    _group_handle: Option<&ProcessGroupHandle>,
) {
    if let Some(pid) = child_pid {
        let pgid = Pid::from_raw(pid as i32);
        match killpg(pgid, Signal::SIGTERM) {
            Ok(()) => {
                debug!(pid, "sent SIGTERM to process group");
            }
            Err(nix::errno::Errno::ESRCH) => {
                debug!(pid, "process group already exited");
                return;
            }
            Err(e) => {
                warn!(pid, error = %e, "killpg(SIGTERM) failed, falling back to kill");
                let _ = child.kill().await;
                return;
            }
        }

        let grace = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        match grace {
            Ok(Ok(_status)) => {
                debug!(pid, "child exited after SIGTERM");
            }
            _ => {
                warn!(pid, "child did not exit within 5s, sending SIGKILL");
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
    } else {
        let _ = child.kill().await;
    }
}

pub fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

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
