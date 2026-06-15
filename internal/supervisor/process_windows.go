//go:build windows

package supervisor

import (
	"os/exec"
	"strconv"
)

// setProcGroup is a no-op on Windows (no POSIX process groups).
func setProcGroup(cmd *exec.Cmd) {}

// terminateProcess kills the process and its entire child tree. Windows has no
// process groups, so `proc.Kill()` would leave any grandchildren (e.g. a dev
// server's spawned workers) orphaned. taskkill /T walks the tree; /F forces it.
func terminateProcess(cmd *exec.Cmd) {
	if cmd.Process == nil {
		return
	}
	pid := cmd.Process.Pid
	if err := exec.Command("taskkill", "/T", "/F", "/PID", strconv.Itoa(pid)).Run(); err != nil {
		// Fall back to killing just the top process if taskkill is unavailable.
		_ = cmd.Process.Kill()
	}
}
