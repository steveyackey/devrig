package tools

import "github.com/steveyackey/devrig/internal/verbose"

// VerboseFlags returns the tool's own verbosity flags when devrig's verbose
// mode is on, so the underlying CLI also logs at a higher level. It returns nil
// otherwise. The flags are global/persistent for each tool, so they are safe to
// append to any subcommand invocation.
func VerboseFlags(t Tool) []string {
	if !verbose.Enabled() {
		return nil
	}
	switch t {
	case K3d:
		return []string{"--verbose"}
	case Helm:
		return []string{"--debug"}
	case Kubectl:
		return []string{"-v=6"}
	}
	return nil
}
