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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_shell_returns_shell_env() {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        assert_eq!(user_shell(), shell);
    }

    #[test]
    fn shell_name_contains_login_flag() {
        let name = shell_name();
        assert!(
            name.ends_with("-lc"),
            "shell_name should end with -lc, got: {}",
            name
        );
    }

    #[test]
    fn shell_name_contains_user_shell_path() {
        let expected = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        let name = shell_name();
        assert!(
            name.starts_with(&expected),
            "shell_name should start with {}, got: {}",
            expected,
            name
        );
    }

    #[tokio::test]
    async fn shell_command_executes_with_user_shell() {
        // Verify the spawned process runs under $SHELL, not plain sh.
        // In -c mode, $0 is the shell name or path (e.g. "bash", "/bin/bash").
        let expected = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        let expected_basename = std::path::Path::new(&expected)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let mut cmd = shell_command("echo $0");
        let output = cmd.output().await.expect("failed to spawn");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let actual_basename = std::path::Path::new(&stdout)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        assert_eq!(
            actual_basename, expected_basename,
            "spawned shell should be {} (from $SHELL), got: {}",
            expected_basename, stdout
        );
    }

    #[tokio::test]
    async fn shell_command_login_shell_has_path() {
        // Login shell (-l) should source profile, giving a populated PATH.
        let mut cmd = shell_command("echo $PATH");
        let output = cmd.output().await.expect("failed to spawn");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        assert!(
            !stdout.is_empty(),
            "login shell PATH should not be empty"
        );
        // PATH from a login shell should contain at least /usr/bin or /bin
        assert!(
            stdout.contains("/bin"),
            "login shell PATH should contain /bin, got: {}",
            stdout
        );
    }
}
