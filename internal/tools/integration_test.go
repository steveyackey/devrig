//go:build toolsintegration

// Integration test: actually downloads the pinned tools, verifies their
// checksums (done inside fetch), and runs each binary to confirm the reported
// version matches the pin. This validates that the pinned versions, download
// URLs, and checksums are all correct and the binaries execute.
//
// Run with: go test -tags toolsintegration ./internal/tools/
package tools

import (
	"context"
	"os/exec"
	"strings"
	"testing"
)

func TestFetchAndRunPinned(t *testing.T) {
	ctx := context.Background()
	r := NewResolver(Options{Dir: t.TempDir(), AllowFetch: true})

	cases := map[Tool][]string{
		Kubectl: {"version", "--client"},
		Helm:    {"version"},
		K3d:     {"version"},
	}

	for _, tool := range All {
		if !SupportedPlatform(tool) {
			t.Logf("%s: no managed build for this platform, skipping", tool)
			continue
		}
		t.Run(string(tool), func(t *testing.T) {
			bin, err := r.Install(ctx, tool, true)
			if err != nil {
				t.Fatalf("install %s: %v", tool, err)
			}
			out, err := exec.CommandContext(ctx, bin, cases[tool]...).CombinedOutput()
			if err != nil {
				t.Fatalf("%s %v: %v\n%s", tool, cases[tool], err, out)
			}
			want := pinnedVersion[tool]
			if !strings.Contains(string(out), want) {
				t.Errorf("%s output does not mention pinned version %q:\n%s", tool, want, out)
			}
		})
	}
}
