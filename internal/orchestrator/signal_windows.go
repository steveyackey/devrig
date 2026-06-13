//go:build windows

package orchestrator

import "os"

// shutdownSignals on Windows: only Interrupt (Ctrl+C) is reliably received.
var shutdownSignals = []os.Signal{os.Interrupt}

// sendStop kills the process (Windows has no SIGTERM).
func sendStop(proc *os.Process) error {
	return proc.Kill()
}

// isAlive checks if a process is running by trying to open it.
// On Windows, FindProcess always succeeds; we use Kill(0) analogue via
// a separate technique — just try to Kill with 0; os.Process doesn't
// support Signal(0) on Windows, so we fall back to always returning false
// (which causes Stop() to proceed as if the process already exited).
func isAlive(proc *os.Process) bool {
	return false
}
