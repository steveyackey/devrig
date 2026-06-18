package verbose

import (
	"bytes"
	"os/exec"
	"runtime"
	"strings"
	"testing"
)

func skipOnWindows(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("uses sh; not available on Windows")
	}
}

func TestRunCombinesOutput(t *testing.T) {
	skipOnWindows(t)
	t.Setenv(EnvVar, "") // verbose off

	got, err := Run(exec.Command("sh", "-c", "echo out; echo err 1>&2"))
	if err != nil {
		t.Fatalf("Run error: %v", err)
	}
	if !strings.Contains(got, "out") || !strings.Contains(got, "err") {
		t.Errorf("combined output = %q, want both stdout and stderr", got)
	}
}

func TestRunStreamsWhenVerbose(t *testing.T) {
	skipOnWindows(t)
	t.Setenv(EnvVar, "1") // verbose on

	var sink bytes.Buffer
	prev := out
	out = &sink
	t.Cleanup(func() { out = prev })

	got, err := Run(exec.Command("sh", "-c", "echo streamed"))
	if err != nil {
		t.Fatalf("Run error: %v", err)
	}
	if !strings.Contains(got, "streamed") {
		t.Errorf("returned output = %q, want it captured too", got)
	}
	if !strings.Contains(sink.String(), "streamed") {
		t.Errorf("live sink = %q, want streamed output mirrored", sink.String())
	}
}

func TestRunSplitSeparatesStreams(t *testing.T) {
	skipOnWindows(t)
	t.Setenv(EnvVar, "") // verbose off

	stdout, stderr, err := RunSplit(exec.Command("sh", "-c", "echo only-out; echo only-err 1>&2"))
	if err != nil {
		t.Fatalf("RunSplit error: %v", err)
	}
	if stdout != "only-out" {
		t.Errorf("stdout = %q, want %q", stdout, "only-out")
	}
	if stderr != "only-err" {
		t.Errorf("stderr = %q, want %q", stderr, "only-err")
	}
}
