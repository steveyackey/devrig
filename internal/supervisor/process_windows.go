//go:build windows

package supervisor

import "os/exec"

// setProcGroup is a no-op on Windows (no process group).
func setProcGroup(cmd *exec.Cmd) {}

// terminateProcess kills the process on Windows.
func terminateProcess(cmd *exec.Cmd) {
	if cmd.Process != nil {
		_ = cmd.Process.Kill()
	}
}
