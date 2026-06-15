//go:build toolssmoke

// Smoke test: exercises devrig's real cluster code paths against a throwaway
// k3d cluster, using devrig-managed k3d/kubectl/helm binaries. This validates
// that the pinned tool versions actually work together — k3d creates a cluster,
// kubectl applies a manifest, and helm installs a chart through addon.go's
// helm-3 argument lists. Requires Docker.
//
// Run with: go test -tags toolssmoke -timeout 15m ./internal/cluster/
package cluster_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/steveyackey/devrig/internal/cluster"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/tools"
)

func TestClusterSmokeWithManagedTools(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 12*time.Minute)
	defer cancel()

	binDir := t.TempDir()
	stateDir := t.TempDir()
	resolver := tools.NewResolver(tools.Options{Dir: binDir, AllowFetch: true})

	// Pre-fetch all managed tools (also fails fast on a bad pin/checksum).
	for _, tool := range tools.All {
		if _, err := resolver.Install(ctx, tool, true); err != nil {
			t.Fatalf("install %s: %v", tool, err)
		}
	}

	clCfg := &config.ClusterConfig{Agents: 0, Registry: false}
	mgr := cluster.NewManager(clCfg, resolver, "smoke", stateDir, stateDir, "devrig-smoke-net")

	cs, err := mgr.Ensure(ctx)
	if err != nil {
		t.Fatalf("cluster create: %v", err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Minute)
		defer cancel()
		if err := mgr.Delete(ctx); err != nil {
			t.Logf("cluster delete (cleanup): %v", err)
		}
	})

	// kubectl apply a trivial manifest through devrig's kubectl path.
	manifest := filepath.Join(stateDir, "ns.yaml")
	if err := os.WriteFile(manifest, []byte("apiVersion: v1\nkind: Namespace\nmetadata:\n  name: smoke\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := cluster.KubectlApply(ctx, resolver, cs.KubeconfigPath, manifest); err != nil {
		t.Fatalf("kubectl apply: %v", err)
	}

	// helm install through devrig's addon.go (exercises the helm-3 arg lists).
	addons := map[string]config.AddonConfig{
		"podinfo": {
			Type:      "helm",
			Chart:     "oci://ghcr.io/stefanprodan/charts/podinfo",
			Namespace: "podinfo",
			Wait:      true,
			Timeout:   "3m",
		},
	}
	if err := cluster.InstallAddons(ctx, resolver, addons, cs, stateDir, stateDir); err != nil {
		t.Fatalf("helm addon install: %v", err)
	}

	// Confirm the release landed, using the managed kubectl.
	kubectl, err := resolver.Path(ctx, tools.Kubectl)
	if err != nil {
		t.Fatal(err)
	}
	cmd := exec.CommandContext(ctx, kubectl, "get", "deploy", "-n", "podinfo", "-o", "name")
	cmd.Env = append(os.Environ(), "KUBECONFIG="+cs.KubeconfigPath)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("kubectl get deploy: %v\n%s", err, out)
	}
	if len(out) == 0 {
		t.Error("expected a podinfo deployment after helm install")
	}
}
