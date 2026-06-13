//go:build !windows

package supervisor

import (
	"os/exec"
	"syscall"
	"time"
)

// setProcGroup puts the process in its own process group so we can kill the whole tree.
func setProcGroup(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

// terminateProcess sends SIGTERM to the process group, then SIGKILL after 5s.
func terminateProcess(cmd *exec.Cmd) {
	if cmd.Process == nil {
		return
	}
	pgid, err := syscall.Getpgid(cmd.Process.Pid)
	if err == nil {
		_ = syscall.Kill(-pgid, syscall.SIGTERM)
		go func() {
			time.Sleep(5 * time.Second)
			_ = syscall.Kill(-pgid, syscall.SIGKILL)
		}()
	} else {
		_ = cmd.Process.Signal(syscall.SIGTERM)
	}
}
