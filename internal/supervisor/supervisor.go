// Package supervisor manages the lifecycle of a single supervised service
// process: spawning, log capture, restart policy, and graceful cancellation.
package supervisor

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"math"
	"math/rand/v2"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/events"
	"github.com/steveyackey/devrig/internal/state"
)

const multilineFlushTimeout = 250 * time.Millisecond

// RestartMode determines when a process should be restarted.
type RestartMode int

const (
	RestartOnFailure RestartMode = iota // restart only on non-zero exit
	RestartAlways                       // restart regardless of exit code
	RestartNever                        // never restart
)

// RestartPolicy holds all tuning knobs for the restart loop.
type RestartPolicy struct {
	Mode               RestartMode
	MaxRestarts        uint32
	StartupMaxRestarts uint32
	StartupGrace       time.Duration
	InitialDelay       time.Duration
	MaxDelay           time.Duration
	ResetAfter         time.Duration
}

// DefaultPolicy matches the defaults in the Rust implementation.
var DefaultPolicy = RestartPolicy{
	Mode:               RestartOnFailure,
	MaxRestarts:        10,
	StartupMaxRestarts: 3,
	StartupGrace:       2 * time.Second,
	InitialDelay:       500 * time.Millisecond,
	MaxDelay:           30 * time.Second,
	ResetAfter:         60 * time.Second,
}

// PolicyFromConfig converts a config RestartConfig to a RestartPolicy.
func PolicyFromConfig(cfg *config.RestartConfig) RestartPolicy {
	if cfg == nil {
		return DefaultPolicy
	}
	mode := RestartOnFailure
	switch cfg.Policy {
	case "always":
		mode = RestartAlways
	case "never":
		mode = RestartNever
	}
	return RestartPolicy{
		Mode:               mode,
		MaxRestarts:        cfg.MaxRestarts,
		StartupMaxRestarts: cfg.StartupMaxRestarts,
		StartupGrace:       time.Duration(cfg.StartupGraceMs) * time.Millisecond,
		InitialDelay:       time.Duration(cfg.InitialDelayMs) * time.Millisecond,
		MaxDelay:           time.Duration(cfg.MaxDelayMs) * time.Millisecond,
		ResetAfter:         60 * time.Second,
	}
}

// Supervisor manages a single service process.
type Supervisor struct {
	Name       string
	Command    string
	WorkingDir string
	Env        map[string]string
	Policy     RestartPolicy

	logBroadcast *events.Broadcaster
	eventBroadcast *events.Broadcaster
	stateDir     string
}

// New creates a Supervisor.
func New(
	name, command, workingDir string,
	env map[string]string,
	policy RestartPolicy,
	logs *events.Broadcaster,
	evts *events.Broadcaster,
	stateDir string,
) *Supervisor {
	return &Supervisor{
		Name:           name,
		Command:        command,
		WorkingDir:     workingDir,
		Env:            env,
		Policy:         policy,
		logBroadcast:   logs,
		eventBroadcast: evts,
		stateDir:       stateDir,
	}
}

// Run supervises the process until ctx is cancelled or the restart budget is
// exhausted. The returned error is non-nil only if the process could not be
// spawned at all.
func (s *Supervisor) Run(ctx context.Context) error {
	var (
		restartCount        uint32
		startupRestartCount uint32
		graceFired          bool
		// recent crash timestamps for rate detection
		recentCrashes []time.Time
	)

	for {
		if ctx.Err() != nil {
			return nil
		}

		spawnTime := time.Now()

		cmd := buildCmd(s.Command, s.WorkingDir, s.Env)
		stdoutPipe, _ := cmd.StdoutPipe()
		stderrPipe, _ := cmd.StderrPipe()

		if err := cmd.Start(); err != nil {
			return fmt.Errorf("spawn %s: %w", s.Name, err)
		}

		pid := uint32(cmd.Process.Pid)
		if s.stateDir != "" {
			state.UpdateServicePID(s.stateDir, s.Name, pid)
		}

		// Start log stream readers.
		var wg sync.WaitGroup
		wg.Add(2)
		go func() { defer wg.Done(); s.streamLogs(ctx, stdoutPipe, false) }()
		go func() { defer wg.Done(); s.streamLogs(ctx, stderrPipe, true) }()

		// Startup grace timer: fires once after StartupGrace to mark "running".
		if !graceFired {
			graceCtx, graceCancel := context.WithCancel(ctx)
			go func() {
				defer graceCancel()
				select {
				case <-time.After(s.Policy.StartupGrace):
					graceFired = true
					if s.stateDir != "" {
						state.UpdateServicePhase(s.stateDir, s.Name, "running")
					}
					if s.eventBroadcast != nil {
						s.eventBroadcast.Send(events.TelemetryEvent{
							Kind:    events.KindServiceStatusChange,
							Service: s.Name,
							Status:  "running",
						})
					}
				case <-graceCtx.Done():
				}
			}()
			// Cancel grace timer when the next iteration starts or ctx is done.
			defer graceCancel()
		}

		// Wait for process exit or context cancellation.
		done := make(chan error, 1)
		go func() { done <- cmd.Wait() }()

		var waitErr error
		select {
		case waitErr = <-done:
			wg.Wait()
		case <-ctx.Done():
			// Terminate the process group.
			terminateProcess(cmd)
			<-done
			wg.Wait()
			return nil
		}

		runtime := time.Since(spawnTime)
		exitCode := exitCodeOf(cmd)

		if s.stateDir != "" {
			state.UpdateServiceExit(s.stateDir, s.Name, exitCode)
		}

		_ = waitErr // process exit is not itself an error

		// RestartNever: always stop.
		if s.Policy.Mode == RestartNever {
			return nil
		}

		// RestartOnFailure: exit 0 → clean exit, don't restart.
		if s.Policy.Mode == RestartOnFailure && exitCode == 0 {
			return nil
		}

		// Mark "running" retroactively if it survived startup grace.
		if !graceFired && runtime >= s.Policy.StartupGrace {
			graceFired = true
		}

		isStartupFailure := runtime < s.Policy.StartupGrace

		// Prune crashes older than 30s and add this one.
		now := time.Now()
		fresh := recentCrashes[:0]
		for _, t := range recentCrashes {
			if now.Sub(t) <= 30*time.Second {
				fresh = append(fresh, t)
			}
		}
		fresh = append(fresh, now)
		recentCrashes = fresh

		// Rapid crash guard: 5 in 30s → give up.
		if len(recentCrashes) >= 5 {
			return fmt.Errorf("service %s: rapid crash loop (5 crashes in 30s)", s.Name)
		}

		// Reset counters if process was healthy long enough.
		if runtime >= s.Policy.ResetAfter {
			restartCount = 0
			startupRestartCount = 0
		}

		// Check restart budgets.
		var budget uint32
		if isStartupFailure {
			startupRestartCount++
			if startupRestartCount > s.Policy.StartupMaxRestarts {
				return fmt.Errorf("service %s: startup failed %d times", s.Name, s.Policy.StartupMaxRestarts)
			}
			budget = startupRestartCount
		} else {
			startupRestartCount = 0
			budget = restartCount
		}

		if restartCount >= s.Policy.MaxRestarts {
			return fmt.Errorf("service %s: crashed %d times", s.Name, s.Policy.MaxRestarts)
		}

		delay := backoffDelay(s.Policy.InitialDelay, s.Policy.MaxDelay, budget)

		select {
		case <-time.After(delay):
		case <-ctx.Done():
			return nil
		}

		restartCount++
	}
}

// streamLogs reads from r line by line, groups multiline entries (continuation
// lines starting with whitespace), and broadcasts them.
func (s *Supervisor) streamLogs(ctx context.Context, r io.Reader, isStderr bool) {
	scanner := bufio.NewScanner(r)
	var buf []string
	flushTimer := time.NewTimer(multilineFlushTimeout)
	defer flushTimer.Stop()

	flush := func() {
		if len(buf) == 0 {
			return
		}
		text := strings.Join(buf, "\n")
		if s.logBroadcast != nil {
			s.logBroadcast.Send(events.TelemetryEvent{
				Kind:        events.KindLogRecord,
				Service:     s.Name,
				LogBody:     text,
				LogSeverity: events.DetectLogLevel(text),
			})
		}
		buf = buf[:0]
	}

	lineCh := make(chan string, 64)
	go func() {
		for scanner.Scan() {
			lineCh <- scanner.Text()
		}
		close(lineCh)
	}()

	for {
		select {
		case line, ok := <-lineCh:
			if !ok {
				flush()
				return
			}
			if isLogEntryStart(line) {
				flush()
			}
			buf = append(buf, line)
			if !flushTimer.Stop() {
				select {
				case <-flushTimer.C:
				default:
				}
			}
			flushTimer.Reset(multilineFlushTimeout)

		case <-flushTimer.C:
			flush()

		case <-ctx.Done():
			flush()
			return
		}
	}
}

func isLogEntryStart(line string) bool {
	return !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "\t")
}

// backoffDelay computes an equal-jitter exponential backoff.
func backoffDelay(initial, max time.Duration, count uint32) time.Duration {
	base := float64(initial.Milliseconds()) * math.Pow(2, float64(count))
	capped := math.Min(base, float64(max.Milliseconds()))
	half := capped / 2
	jitter := rand.Float64() * half
	return time.Duration(half+jitter) * time.Millisecond
}

func exitCodeOf(cmd *exec.Cmd) int {
	if cmd.ProcessState == nil {
		return -1
	}
	return cmd.ProcessState.ExitCode()
}

// buildCmd constructs a shell command for the given command string.
func buildCmd(command, workingDir string, env map[string]string) *exec.Cmd {
	var cmd *exec.Cmd
	if runtime.GOOS == "windows" {
		cmd = exec.Command("cmd", "/C", command)
	} else {
		cmd = exec.Command("sh", "-c", command)
	}

	if workingDir != "" {
		cmd.Dir = workingDir
	}

	// Inherit parent env, then overlay service env.
	cmd.Env = os.Environ()
	for k, v := range env {
		cmd.Env = append(cmd.Env, k+"="+v)
	}

	// Create a new process group so we can kill the whole tree.
	setProcGroup(cmd)

	return cmd
}

// PIDFilePath returns the path to the devrig master PID file.
func PIDFilePath(stateDir string) string {
	return filepath.Join(stateDir, "devrig.pid")
}

// WritePIDFile writes the current process PID to the state directory.
func WritePIDFile(stateDir string) error {
	if err := os.MkdirAll(stateDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(PIDFilePath(stateDir), []byte(fmt.Sprint(os.Getpid())), 0o644)
}

// RemovePIDFile removes the PID file.
func RemovePIDFile(stateDir string) {
	_ = os.Remove(PIDFilePath(stateDir))
}
