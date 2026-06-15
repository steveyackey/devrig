package supervisor

import (
	"os"
	"os/exec"
	"testing"
	"time"
)

func TestProcStartTimeMsSelf(t *testing.T) {
	ms, ok := ProcStartTimeMs(os.Getpid())
	if !ok {
		t.Fatal("expected to read own start time")
	}
	if ms <= 0 {
		t.Errorf("start time = %d, want > 0", ms)
	}
}

func TestSameProcess(t *testing.T) {
	pid := os.Getpid()
	ms, ok := ProcStartTimeMs(pid)
	if !ok {
		t.Fatal("ProcStartTimeMs failed")
	}
	if !SameProcess(pid, ms) {
		t.Error("SameProcess should match self with correct start time")
	}
	// Mismatched start time => PID reuse => not the same process.
	if SameProcess(pid, ms+1_000_000) {
		t.Error("SameProcess should reject a mismatched start time")
	}
	// Zero start time => unknown => liveness-only, should match a live PID.
	if !SameProcess(pid, 0) {
		t.Error("SameProcess with unknown start time should fall back to liveness")
	}
}

func TestSameProcessDeadPID(t *testing.T) {
	// Spawn and reap a process, then confirm its PID no longer matches.
	cmd := exec.Command("sleep", "30")
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot spawn sleep: %v", err)
	}
	pid := cmd.Process.Pid
	ms, ok := ProcStartTimeMs(pid)
	if !ok {
		t.Fatal("expected start time for live child")
	}
	if !SameProcess(pid, ms) {
		t.Error("live child should match")
	}
	_ = cmd.Process.Kill()
	_, _ = cmd.Process.Wait()
	// After exit, identity should no longer hold.
	if SameProcess(pid, ms) {
		t.Error("dead child should not match")
	}
}

func TestKillTree(t *testing.T) {
	cmd := exec.Command("sleep", "60")
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot spawn sleep: %v", err)
	}
	pid := cmd.Process.Pid
	KillTree(pid, 500*time.Millisecond)
	_, _ = cmd.Process.Wait()
	if SameProcess(pid, 0) {
		t.Error("process should be dead after KillTree")
	}
}
