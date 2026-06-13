package orchestrator

import (
	"context"
	"os/exec"
	"runtime"
)

// buildDevCmd returns an exec.Cmd for `pnpm dev` (falling back to bun/npm)
// in the given directory, or nil if no package manager is found.
func buildDevCmd(ctx context.Context, webDir string) *exec.Cmd {
	suffix := ""
	if runtime.GOOS == "windows" {
		suffix = ".cmd"
	}
	for _, pm := range []string{"pnpm", "bun", "npm"} {
		bin := pm + suffix
		if _, err := exec.LookPath(bin); err == nil {
			cmd := exec.CommandContext(ctx, bin, "run", "dev")
			cmd.Dir = webDir
			return cmd
		}
	}
	return nil
}
