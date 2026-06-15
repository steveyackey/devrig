//go:build windows

package orchestrator

import (
	"os"
	"os/exec"
	"strconv"

	"golang.org/x/sys/windows"
)

// shutdownSignals on Windows: only Interrupt (Ctrl+C) is reliably received.
var shutdownSignals = []os.Signal{os.Interrupt}

// sendStop terminates the devrig process and its whole child tree. Windows has
// no SIGTERM and can't deliver a catchable signal to another process, so this
// is a forceful stop (unlike the graceful SIGTERM on Unix). taskkill /T tears
// down the tree so the supervised services aren't orphaned; /F forces it.
func sendStop(proc *os.Process) error {
	if err := exec.Command("taskkill", "/T", "/F", "/PID", strconv.Itoa(proc.Pid)).Run(); err != nil {
		// Fall back to killing just the devrig process.
		return proc.Kill()
	}
	return nil
}

// isAlive reports whether the process is still running by opening a handle and
// checking its exit code. FindProcess always succeeds on Windows, so we query
// the OS directly.
func isAlive(proc *os.Process) bool {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(proc.Pid))
	if err != nil {
		return false
	}
	defer windows.CloseHandle(h)
	var code uint32
	if err := windows.GetExitCodeProcess(h, &code); err != nil {
		return false
	}
	const stillActive = 259 // STILL_ACTIVE
	return code == stillActive
}
