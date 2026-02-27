use std::path::PathBuf;
use tokio::process::Command;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

pub use imp::ProcessGroupHandle;

/// Create a platform-appropriate shell command.
/// Unix: `$SHELL -l -c <command>`, Windows: `cmd.exe /C <command>`
pub fn shell_command(command: &str) -> Command {
    imp::shell_command(command)
}

/// Configure the command to run in a new process group.
/// Unix: `process_group(0)`, Windows: `CREATE_NEW_PROCESS_GROUP`
pub fn configure_process_group(cmd: &mut Command) {
    imp::configure_process_group(cmd)
}

/// Perform any post-spawn setup (e.g., Job Object on Windows).
/// Returns a handle that must be kept alive for the process lifetime.
pub fn post_spawn_setup(child_pid: Option<u32>) -> Option<ProcessGroupHandle> {
    imp::post_spawn_setup(child_pid)
}

/// Gracefully terminate a child process and its descendants.
/// Tries graceful shutdown first, then forcefully kills after 5 seconds.
pub async fn terminate_child(
    child: &mut tokio::process::Child,
    child_pid: Option<u32>,
    group_handle: Option<&ProcessGroupHandle>,
) {
    imp::terminate_child(child, child_pid, group_handle).await
}

/// Check if a process with the given PID is still alive.
pub fn is_process_alive(pid: u32) -> bool {
    imp::is_process_alive(pid)
}

/// Get the current user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Expand leading `~` or `$HOME` in a path string to the actual home directory.
///
/// Returns the original string unchanged when no home directory is available
/// or the string doesn't start with `~` or `$HOME`.
pub fn expand_home(path: &str) -> String {
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
    {
        let home = home.to_string_lossy();
        if path == "~" || path == "$HOME" {
            return home.to_string();
        }
        if path.starts_with("~/") {
            return format!("{}{}", home, &path[1..]);
        }
        if path.starts_with("$HOME/") || path.starts_with("$HOME\\") {
            return format!("{}{}", home, &path[5..]);
        }
    }
    path.to_string()
}

/// Identify which process owns a given TCP port.
pub fn identify_port_owner(port: u16) -> Option<String> {
    imp::identify_port_owner(port)
}

/// Shell name for log messages.
pub fn shell_name() -> String {
    imp::shell_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_tilde_slash() {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap();
        assert_eq!(expand_home("~/bin/cmd"), format!("{}/bin/cmd", home));
    }

    #[test]
    fn expand_home_bare_tilde() {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap();
        assert_eq!(expand_home("~"), home);
    }

    #[test]
    fn expand_home_dollar_home() {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap();
        assert_eq!(expand_home("$HOME"), home);
        assert_eq!(expand_home("$HOME/projects"), format!("{}/projects", home));
    }

    #[test]
    fn expand_home_no_expansion_needed() {
        assert_eq!(expand_home("/usr/bin/cmd"), "/usr/bin/cmd");
        assert_eq!(expand_home("relative/path"), "relative/path");
        assert_eq!(expand_home(""), "");
    }

    #[test]
    fn expand_home_tilde_not_at_start() {
        // Tilde in the middle should NOT be expanded
        assert_eq!(expand_home("/some/~path"), "/some/~path");
    }
}

#[cfg(test)]
pub mod test_commands {
    #[cfg(unix)]
    pub fn echo_two_lines() -> &'static str {
        "echo hello && echo world"
    }
    #[cfg(windows)]
    pub fn echo_two_lines() -> &'static str {
        "echo hello&& echo world"
    }

    #[cfg(unix)]
    pub fn echo_stderr() -> &'static str {
        "echo err >&2"
    }
    #[cfg(windows)]
    pub fn echo_stderr() -> &'static str {
        "echo err>&2"
    }

    #[cfg(unix)]
    pub fn sleep_long() -> &'static str {
        "sleep 60"
    }
    #[cfg(windows)]
    pub fn sleep_long() -> &'static str {
        // `timeout` exits immediately when stdout is piped (non-interactive).
        // `ping` with 61 attempts (~1s each) reliably blocks for ~60s.
        "ping -n 61 127.0.0.1 > nul"
    }

    #[cfg(unix)]
    pub fn exit_success() -> &'static str {
        "exit 0"
    }
    #[cfg(windows)]
    pub fn exit_success() -> &'static str {
        "exit /b 0"
    }

    #[cfg(unix)]
    pub fn exit_failure() -> &'static str {
        "exit 1"
    }
    #[cfg(windows)]
    pub fn exit_failure() -> &'static str {
        "exit /b 1"
    }
}
