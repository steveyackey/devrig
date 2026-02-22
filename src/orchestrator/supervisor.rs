use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::config::model::RestartConfig;
use crate::ui::logs::LogLine;

#[cfg(unix)]
use nix::sys::signal::{killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

// ---------------------------------------------------------------------------
// ServicePhase — explicit state tracking for supervisor lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServicePhase {
    Initial,
    Starting,
    Running,
    Backoff { attempt: u32 },
    Failed { reason: String },
    Stopped,
}

// ---------------------------------------------------------------------------
// RestartMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartMode {
    Always,
    OnFailure,
    Never,
}

impl RestartMode {
    fn from_str(s: &str) -> Self {
        match s {
            "always" => RestartMode::Always,
            "never" => RestartMode::Never,
            _ => RestartMode::OnFailure,
        }
    }
}

// ---------------------------------------------------------------------------
// RestartPolicy
// ---------------------------------------------------------------------------

pub struct RestartPolicy {
    pub max_restarts: u32,
    pub startup_max_restarts: u32,
    pub startup_grace: Duration,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub reset_after: Duration,
    pub mode: RestartMode,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 10,
            startup_max_restarts: 3,
            startup_grace: Duration::from_secs(2),
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            reset_after: Duration::from_secs(60),
            mode: RestartMode::OnFailure,
        }
    }
}

impl RestartPolicy {
    /// Construct a RestartPolicy from the TOML RestartConfig.
    pub fn from_config(cfg: &RestartConfig) -> Self {
        Self {
            max_restarts: cfg.max_restarts,
            startup_max_restarts: cfg.startup_max_restarts,
            startup_grace: Duration::from_millis(cfg.startup_grace_ms),
            initial_delay: Duration::from_millis(cfg.initial_delay_ms),
            max_delay: Duration::from_millis(cfg.max_delay_ms),
            reset_after: Duration::from_secs(60),
            mode: RestartMode::from_str(&cfg.policy),
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
    log_tx: broadcast::Sender<LogLine>,
    cancel: CancellationToken,
}

impl ServiceSupervisor {
    pub fn new(
        name: String,
        command: String,
        working_dir: Option<PathBuf>,
        env: BTreeMap<String, String>,
        policy: RestartPolicy,
        log_tx: broadcast::Sender<LogLine>,
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
        let mut startup_restart_count: u32 = 0;
        let mut last_status: Option<ExitStatus> = None;
        let mut _phase = ServicePhase::Initial;

        // Track recent crash timestamps for crash rate detection
        let mut recent_crashes: VecDeque<Instant> = VecDeque::new();

        loop {
            // Check cancellation before (re)spawning.
            if self.cancel.is_cancelled() {
                _phase = ServicePhase::Stopped;
                info!(service = %self.name, "cancelled before spawn");
                return last_status.ok_or_else(|| {
                    anyhow::anyhow!("service {} cancelled before first start", self.name)
                });
            }

            _phase = ServicePhase::Starting;
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
                                    let level = crate::ui::logs::detect_log_level(&text);
                                    let _ = tx.send(LogLine {
                                        timestamp: chrono::Utc::now(),
                                        service: svc.clone(),
                                        text,
                                        is_stderr: false,
                                        level,
                                    });
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
                                    let level = crate::ui::logs::detect_log_level(&text);
                                    let _ = tx.send(LogLine {
                                        timestamp: chrono::Utc::now(),
                                        service: svc.clone(),
                                        text,
                                        is_stderr: true,
                                        level,
                                    });
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
                    _phase = ServicePhase::Stopped;
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
            let runtime = spawn_time.elapsed();

            info!(
                service = %self.name,
                status = %status,
                runtime_ms = runtime.as_millis() as u64,
                "process exited",
            );

            // Determine if this was a startup failure or runtime crash
            let is_startup_failure = runtime < self.policy.startup_grace;
            let exit_code = status.code();

            // RestartMode::Never — don't restart at all
            if self.policy.mode == RestartMode::Never {
                info!(service = %self.name, "restart mode is 'never', not restarting");
                _phase = ServicePhase::Stopped;
                return Ok(status);
            }

            // RestartMode::OnFailure — exit code 0 means clean exit, don't restart
            if self.policy.mode == RestartMode::OnFailure && exit_code == Some(0) {
                info!(service = %self.name, "clean exit (code 0) with on-failure policy, not restarting");
                _phase = ServicePhase::Stopped;
                return Ok(status);
            }

            // Update phase to Running retroactively if it ran past startup_grace
            if !is_startup_failure {
                _phase = ServicePhase::Running;
            }

            // Track crash for rate detection
            let now = Instant::now();
            recent_crashes.push_back(now);
            // Keep only crashes within the last 30 seconds
            while let Some(&front) = recent_crashes.front() {
                if now.duration_since(front) > Duration::from_secs(30) {
                    recent_crashes.pop_front();
                } else {
                    break;
                }
            }

            // Crash rate detection: 5 crashes in 30s → immediate failure
            if recent_crashes.len() >= 5 {
                error!(
                    service = %self.name,
                    crashes_in_30s = recent_crashes.len(),
                    "rapid crash loop detected, giving up",
                );
                _phase = ServicePhase::Failed {
                    reason: "rapid crash loop (5 crashes in 30s)".to_string(),
                };
                return Ok(status);
            }

            // If the child ran longer than reset_after, reset the restart
            // counter — the service was healthy for a reasonable period.
            if runtime >= self.policy.reset_after {
                debug!(
                    service = %self.name,
                    runtime_secs = runtime.as_secs(),
                    "runtime exceeded reset_after; resetting restart counter",
                );
                restart_count = 0;
                startup_restart_count = 0;
            }

            // Check restart budgets
            let budget = if is_startup_failure {
                startup_restart_count += 1;
                if startup_restart_count > self.policy.startup_max_restarts {
                    error!(
                        service = %self.name,
                        max_startup_restarts = self.policy.startup_max_restarts,
                        "reached maximum startup restart count, giving up",
                    );
                    _phase = ServicePhase::Failed {
                        reason: format!(
                            "startup failed {} times",
                            self.policy.startup_max_restarts
                        ),
                    };
                    return Ok(status);
                }
                startup_restart_count
            } else {
                startup_restart_count = 0; // Reset startup counter on runtime failures
                restart_count
            };

            if restart_count >= self.policy.max_restarts {
                error!(
                    service = %self.name,
                    max_restarts = self.policy.max_restarts,
                    "reached maximum restart count, giving up",
                );
                _phase = ServicePhase::Failed {
                    reason: format!("crashed {} times", self.policy.max_restarts),
                };
                return Ok(status);
            }

            // Compute exponential backoff with equal jitter.
            let delay = Self::backoff_delay(&self.policy, budget);
            _phase = ServicePhase::Backoff {
                attempt: restart_count + 1,
            };
            info!(
                service = %self.name,
                delay_ms = delay.as_millis() as u64,
                restart_count,
                startup_failure = is_startup_failure,
                "restarting after backoff",
            );

            // Sleep with cancellation awareness.
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.cancel.cancelled() => {
                    _phase = ServicePhase::Stopped;
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
        assert_eq!(p.startup_max_restarts, 3);
        assert_eq!(p.startup_grace, Duration::from_secs(2));
        assert_eq!(p.initial_delay, Duration::from_millis(500));
        assert_eq!(p.max_delay, Duration::from_secs(30));
        assert_eq!(p.reset_after, Duration::from_secs(60));
        assert_eq!(p.mode, RestartMode::OnFailure);
    }

    #[test]
    fn backoff_delay_stays_within_bounds() {
        let policy = RestartPolicy {
            max_restarts: 20,
            startup_max_restarts: 3,
            startup_grace: Duration::from_secs(2),
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            reset_after: Duration::from_secs(60),
            mode: RestartMode::OnFailure,
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
        let (tx, mut rx) = broadcast::channel::<LogLine>(64);
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
        let (tx, mut rx) = broadcast::channel::<LogLine>(64);
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
        let (tx, _rx) = broadcast::channel::<LogLine>(64);
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

    #[tokio::test]
    async fn exit_code_zero_with_on_failure_no_restart() {
        let (tx, _rx) = broadcast::channel::<LogLine>(64);
        let cancel = CancellationToken::new();

        let supervisor = ServiceSupervisor::new(
            "test-clean-exit".into(),
            "exit 0".into(),
            None,
            BTreeMap::new(),
            RestartPolicy {
                max_restarts: 10,
                mode: RestartMode::OnFailure,
                ..RestartPolicy::default()
            },
            tx,
            cancel,
        );

        let status = supervisor.run().await.expect("run should succeed");
        assert!(status.success());
        // Should not restart — should return immediately on exit 0
    }

    #[tokio::test]
    async fn restart_mode_never_no_restart() {
        let (tx, _rx) = broadcast::channel::<LogLine>(64);
        let cancel = CancellationToken::new();

        let supervisor = ServiceSupervisor::new(
            "test-never".into(),
            "exit 1".into(),
            None,
            BTreeMap::new(),
            RestartPolicy {
                max_restarts: 10,
                mode: RestartMode::Never,
                ..RestartPolicy::default()
            },
            tx,
            cancel,
        );

        let status = supervisor.run().await.expect("run should succeed");
        assert!(!status.success());
        // Should not restart even though exit code is non-zero
    }

    #[test]
    fn service_phase_transitions() {
        let phase = ServicePhase::Initial;
        assert_eq!(phase, ServicePhase::Initial);

        let phase = ServicePhase::Starting;
        assert_eq!(phase, ServicePhase::Starting);

        let phase = ServicePhase::Running;
        assert_eq!(phase, ServicePhase::Running);

        let phase = ServicePhase::Backoff { attempt: 3 };
        assert_eq!(phase, ServicePhase::Backoff { attempt: 3 });

        let phase = ServicePhase::Failed {
            reason: "too many crashes".to_string(),
        };
        assert!(matches!(phase, ServicePhase::Failed { .. }));

        let phase = ServicePhase::Stopped;
        assert_eq!(phase, ServicePhase::Stopped);
    }

    #[test]
    fn restart_mode_from_str() {
        assert_eq!(RestartMode::from_str("always"), RestartMode::Always);
        assert_eq!(RestartMode::from_str("on-failure"), RestartMode::OnFailure);
        assert_eq!(RestartMode::from_str("never"), RestartMode::Never);
        assert_eq!(RestartMode::from_str("unknown"), RestartMode::OnFailure);
    }

    #[test]
    fn restart_policy_from_config() {
        let cfg = RestartConfig {
            policy: "always".to_string(),
            max_restarts: 5,
            startup_max_restarts: 2,
            startup_grace_ms: 3000,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
        };
        let policy = RestartPolicy::from_config(&cfg);
        assert_eq!(policy.max_restarts, 5);
        assert_eq!(policy.startup_max_restarts, 2);
        assert_eq!(policy.startup_grace, Duration::from_millis(3000));
        assert_eq!(policy.initial_delay, Duration::from_millis(1000));
        assert_eq!(policy.max_delay, Duration::from_millis(60000));
        assert_eq!(policy.mode, RestartMode::Always);
    }
}
