//go:build !windows

package orchestrator

import (
	"os"
	"syscall"
)

// shutdownSignals lists the OS signals that trigger a graceful shutdown.
var shutdownSignals = []os.Signal{os.Interrupt, syscall.SIGTERM}

// sendStop sends SIGTERM to the process.
func sendStop(proc *os.Process) error {
	return proc.Signal(syscall.SIGTERM)
}

// isAlive returns true if the process is still running.
func isAlive(proc *os.Process) bool {
	return proc.Signal(syscall.Signal(0)) == nil
}
