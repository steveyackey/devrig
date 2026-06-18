// Package verbose centralizes devrig's --verbose / DEVRIG_VERBOSE mode. When
// enabled, the orchestrator's child processes (k3d, helm, kubectl, docker,
// docker compose) stream their output live to stderr instead of being captured
// silently, so a hanging or failing step shows what it is doing in real time.
//
// State is carried in an environment variable rather than threaded through
// every call: the --verbose flag sets it once at startup, and every code path
// — and every child process devrig spawns — observes the same setting.
package verbose

import (
	"bytes"
	"io"
	"os"
	"os/exec"
	"strings"
)

// EnvVar enables verbose mode when set to a non-empty value.
const EnvVar = "DEVRIG_VERBOSE"

// out is where live output is mirrored; a var so tests can redirect it.
var out io.Writer = os.Stderr

// Enable turns verbose mode on for this process and any children it spawns.
func Enable() { _ = os.Setenv(EnvVar, "1") }

// Enabled reports whether verbose mode is on.
func Enabled() bool { return os.Getenv(EnvVar) != "" }

// Run executes cmd and returns its trimmed combined output (stdout+stderr),
// like exec.Cmd.CombinedOutput. When verbose is enabled it additionally streams
// that output to stderr as the command produces it. The caller may set cmd.Env,
// cmd.Dir, etc. beforehand; Run only assigns the output writers.
func Run(cmd *exec.Cmd) (string, error) {
	var buf bytes.Buffer
	if Enabled() {
		// The same writer value for both streams makes os/exec share a single
		// pipe, so concurrent writes can't race and stay correctly interleaved.
		w := io.MultiWriter(&buf, out)
		cmd.Stdout, cmd.Stderr = w, w
	} else {
		cmd.Stdout, cmd.Stderr = &buf, &buf
	}
	err := cmd.Run()
	return strings.TrimSpace(buf.String()), err
}

// RunSplit executes cmd capturing stdout and stderr separately — for commands
// whose stdout is parsed (JSON, kubeconfig) and must not be polluted by log
// lines on stderr. When verbose is enabled, stderr is also streamed live.
func RunSplit(cmd *exec.Cmd) (stdout, stderr string, err error) {
	var so, se bytes.Buffer
	cmd.Stdout = &so
	if Enabled() {
		cmd.Stderr = io.MultiWriter(&se, out)
	} else {
		cmd.Stderr = &se
	}
	err = cmd.Run()
	return strings.TrimSpace(so.String()), strings.TrimSpace(se.String()), err
}
