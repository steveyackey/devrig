use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::ui::logs::LogLine;

#[cfg(unix)]
use nix::sys::signal::{killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

// ---------------------------------------------------------------------------
// RestartPolicy
// ---------------------------------------------------------------------------

pub struct RestartPolicy {
    pub max_restarts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub reset_after: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 10,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            reset_after: Duration::from_secs(60),
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceSupervisor
// ---------------------------------------------------------------------------

pub struct ServiceSupervisor {
    name: String,
    command: String,
    working_dir: Option<PathBuf>,
    env: BTreeMap<String, String>,
    policy: RestartPolicy,
    log_tx: mpsc::Sender<LogLine>,
    cancel: CancellationToken,
}

impl ServiceSupervisor {
    pub fn new(
        name: String,
        command: String,
        working_dir: Option<PathBuf>,
        env: BTreeMap<String, String>,
        policy: RestartPolicy,
        log_tx: mpsc::Sender<LogLine>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            name,
            command,
            working_dir,
            env,
            policy,
            log_tx,
            cancel,
        }
    }

    /// Runs the supervised process in a loop, restarting on failure according
    /// to the configured [`RestartPolicy`].  Returns the final [`ExitStatus`]
    /// if the process exited, or an error if spawning failed irrecoverably.
    pub async fn run(self) -> Result<ExitStatus> {
        let mut restart_count: u32 = 0;
        let mut last_status: Option<ExitStatus> = None;

        loop {
            // Check cancellation before (re)spawning.
            if self.cancel.is_cancelled() {
                info!(service = %self.name, "cancelled before spawn");
                return last_status.ok_or_else(|| {
                    anyhow::anyhow!("service {} cancelled before first start", self.name)
                });
            }

            info!(
                service = %self.name,
                attempt = restart_count + 1,
                "spawning: sh -c {:?}",
                self.command,
            );

            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&self.command);

            if let Some(ref dir) = self.working_dir {
                cmd.current_dir(dir);
            }

            cmd.envs(&self.env);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            cmd.kill_on_drop(true);

            // On Unix we run in a new process group so we can signal the
            // entire tree with killpg.
            #[cfg(unix)]
            cmd.process_group(0);

            let spawn_time = Instant::now();

            let mut child = cmd
                .spawn()
                .with_context(|| format!("failed to spawn service {}", self.name))?;

            let child_pid = child.id();
            debug!(service = %self.name, pid = ?child_pid, "child spawned");

            // -----------------------------------------------------------
            // Pipe stdout / stderr into the log channel
            // -----------------------------------------------------------
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let stdout_handle = {
                let tx = self.log_tx.clone();
                let svc = self.name.clone();
                tokio::spawn(async move {
                    if let Some(out) = stdout {
                        let mut reader = BufReader::new(out);
                        let mut line = String::new();
                        loop {
                            line.clear();
                            match reader.read_line(&mut line).await {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    let text = line.trim_end_matches('\n').to_string();
                                    let _ = tx
                                        .send(LogLine {
                                            service: svc.clone(),
                                            text,
                                            is_stderr: false,
                                        })
                                        .await;
                                }
                                Err(e) => {
                                    warn!(service = %svc, error = %e, "stdout read error");
                                    break;
                                }
                            }
                        }
                    }
                })
            };

            let stderr_handle = {
                let tx = self.log_tx.clone();
                let svc = self.name.clone();
                tokio::spawn(async move {
                    if let Some(err) = stderr {
                        let mut reader = BufReader::new(err);
                        let mut line = String::new();
                        loop {
                            line.clear();
                            match reader.read_line(&mut line).await {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    let text = line.trim_end_matches('\n').to_string();
                                    let _ = tx
                                        .send(LogLine {
                                            service: svc.clone(),
                                            text,
                                            is_stderr: true,
                                        })
                                        .await;
                                }
                                Err(e) => {
                                    warn!(service = %svc, error = %e, "stderr read error");
                                    break;
                                }
                            }
                        }
                    }
                })
            };

            // -----------------------------------------------------------
            // Wait for exit or cancellation
            // -----------------------------------------------------------
            let status = tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(s) => s,
                        Err(e) => {
                            error!(service = %self.name, error = %e, "wait() failed");
                            return Err(e).context(format!("waiting on service {}", self.name));
                        }
                    }
                }
                _ = self.cancel.cancelled() => {
                    info!(service = %self.name, "cancellation requested, sending SIGTERM to process group");
                    Self::terminate_child(&mut child, child_pid).await;
                    // Drain the IO tasks.
                    let _ = stdout_handle.await;
                    let _ = stderr_handle.await;
                    // Return whatever status we got from wait after kill.
                    return last_status.ok_or_else(|| {
                        anyhow::anyhow!("service {} cancelled", self.name)
                    });
                }
            };

            // Let IO tasks finish draining.
            let _ = stdout_handle.await;
            let _ = stderr_handle.await;

            last_status = Some(status);

            info!(
                service = %self.name,
                status = %status,
                "process exited",
            );

            // If the child ran longer than reset_after, reset the restart
            // counter -- the service was healthy for a reasonable period.
            let runtime = spawn_time.elapsed();
            if runtime >= self.policy.reset_after {
                debug!(
                    service = %self.name,
                    runtime_secs = runtime.as_secs(),
                    "runtime exceeded reset_after; resetting restart counter",
                );
                restart_count = 0;
            }

            // Check if we have exceeded the restart budget.
            if restart_count >= self.policy.max_restarts {
                error!(
                    service = %self.name,
                    max_restarts = self.policy.max_restarts,
                    "reached maximum restart count, giving up",
                );
                return Ok(status);
            }

            // Compute exponential backoff with equal jitter.
            let delay = Self::backoff_delay(&self.policy, restart_count);
            info!(
                service = %self.name,
                delay_ms = delay.as_millis() as u64,
                restart_count,
                "restarting after backoff",
            );

            // Sleep with cancellation awareness.
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.cancel.cancelled() => {
                    info!(service = %self.name, "cancelled during backoff");
                    return last_status.ok_or_else(|| {
                        anyhow::anyhow!("service {} cancelled during backoff", self.name)
                    });
                }
            }

            restart_count += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Computes a backoff duration using equal-jitter exponential backoff.
    fn backoff_delay(policy: &RestartPolicy, restart_count: u32) -> Duration {
        let base_ms = policy.initial_delay.as_millis() as f64 * 2_f64.powi(restart_count as i32);
        let capped_ms = base_ms.min(policy.max_delay.as_millis() as f64);
        let half = capped_ms / 2.0;
        let jitter = rand::random::<f64>() * half;
        Duration::from_millis((half + jitter) as u64)
    }

    /// Attempts to gracefully terminate the child by sending SIGTERM to its
    /// process group, waiting up to 5 seconds, then falling back to
    /// `child.kill()`.
    async fn terminate_child(child: &mut tokio::process::Child, child_pid: Option<u32>) {
        #[cfg(unix)]
        {
            if let Some(pid) = child_pid {
                let pgid = Pid::from_raw(pid as i32);
                match killpg(pgid, Signal::SIGTERM) {
                    Ok(()) => {
                        debug!(pid, "sent SIGTERM to process group");
                    }
                    Err(nix::errno::Errno::ESRCH) => {
                        // Process (group) already gone -- nothing to do.
                        debug!(pid, "process group already exited");
                        return;
                    }
                    Err(e) => {
                        warn!(pid, error = %e, "killpg(SIGTERM) failed, falling back to kill");
                        let _ = child.kill().await;
                        return;
                    }
                }

                // Give the group up to 5 seconds to exit.
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
                // No PID means spawn likely failed; just kill.
                let _ = child.kill().await;
            }
        }

        #[cfg(not(unix))]
        {
            let _ = child_pid;
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_restart_policy() {
        let p = RestartPolicy::default();
        assert_eq!(p.max_restarts, 10);
        assert_eq!(p.initial_delay, Duration::from_millis(500));
        assert_eq!(p.max_delay, Duration::from_secs(30));
        assert_eq!(p.reset_after, Duration::from_secs(60));
    }

    #[test]
    fn backoff_delay_stays_within_bounds() {
        let policy = RestartPolicy {
            max_restarts: 20,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            reset_after: Duration::from_secs(60),
        };

        for count in 0..20 {
            let delay = ServiceSupervisor::backoff_delay(&policy, count);
            // Delay must never exceed max_delay.
            assert!(
                delay <= policy.max_delay,
                "delay {:?} exceeded max {:?} at count {}",
                delay,
                policy.max_delay,
                count,
            );
            // Delay must be at least half the capped base (the non-jitter portion).
            let base_ms = policy.initial_delay.as_millis() as f64 * 2_f64.powi(count as i32);
            let capped_ms = base_ms.min(policy.max_delay.as_millis() as f64);
            let min_expected = Duration::from_millis((capped_ms / 2.0) as u64);
            assert!(
                delay >= min_expected,
                "delay {:?} below minimum {:?} at count {}",
                delay,
                min_expected,
                count,
            );
        }
    }

    #[tokio::test]
    async fn supervisor_runs_and_exits() {
        let (tx, mut rx) = mpsc::channel::<LogLine>(64);
        let cancel = CancellationToken::new();

        let supervisor = ServiceSupervisor::new(
            "test-echo".into(),
            "echo hello && echo world".into(),
            None,
            BTreeMap::new(),
            RestartPolicy {
                max_restarts: 0, // do not restart
                ..RestartPolicy::default()
            },
            tx,
            cancel.clone(),
        );

        let status = supervisor.run().await.expect("run should succeed");
        assert!(status.success());

        // Collect all log lines.
        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }

        assert!(
            lines.iter().any(|l| l.text == "hello"),
            "expected 'hello' in logs, got: {:?}",
            lines,
        );
        assert!(
            lines.iter().any(|l| l.text == "world"),
            "expected 'world' in logs, got: {:?}",
            lines,
        );
        assert!(lines.iter().all(|l| l.service == "test-echo"));
        assert!(lines.iter().all(|l| !l.is_stderr));
    }

    #[tokio::test]
    async fn supervisor_captures_stderr() {
        let (tx, mut rx) = mpsc::channel::<LogLine>(64);
        let cancel = CancellationToken::new();

        let supervisor = ServiceSupervisor::new(
            "test-stderr".into(),
            "echo err >&2".into(),
            None,
            BTreeMap::new(),
            RestartPolicy {
                max_restarts: 0,
                ..RestartPolicy::default()
            },
            tx,
            cancel.clone(),
        );

        let status = supervisor.run().await.expect("run should succeed");
        assert!(status.success());

        let mut lines = Vec::new();
        while let Ok(line) = rx.try_recv() {
            lines.push(line);
        }

        assert!(
            lines.iter().any(|l| l.text == "err" && l.is_stderr),
            "expected stderr line 'err', got: {:?}",
            lines,
        );
    }

    #[tokio::test]
    async fn supervisor_cancel_stops_process() {
        let (tx, _rx) = mpsc::channel::<LogLine>(64);
        let cancel = CancellationToken::new();

        let supervisor = ServiceSupervisor::new(
            "test-cancel".into(),
            "sleep 60".into(),
            None,
            BTreeMap::new(),
            RestartPolicy::default(),
            tx,
            cancel.clone(),
        );

        let handle = tokio::spawn(supervisor.run());

        // Give the process a moment to start.
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("should complete within timeout")
            .expect("task should not panic");

        // After cancel the result is an error (no successful exit to report).
        assert!(
            result.is_err(),
            "expected Err after cancel, got: {:?}",
            result
        );
    }
}
