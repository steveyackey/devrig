package cluster

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"gopkg.in/yaml.v3"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/state"
)

// InstallAddons installs all configured addons in topological order,
// then starts port-forwards. ctx cancellation stops the port-forwards.
func InstallAddons(
	ctx context.Context,
	addons map[string]config.AddonConfig,
	cs *state.ClusterState,
	stateDir string,
) error {
	order, err := addonTopoSort(addons)
	if err != nil {
		return fmt.Errorf("addon install: %w", err)
	}

	for _, name := range order {
		addon := addons[name]
		if err := installAddon(ctx, name, &addon, cs); err != nil {
			return fmt.Errorf("addon %s: %w", name, err)
		}
		ns := addon.Namespace
		if ns == "" {
			ns = "default"
		}
		cs.InstalledAddons[name] = state.AddonState{
			AddonType:   addon.Type,
			Namespace:   ns,
			InstalledAt: time.Now(),
		}
	}

	// Start port-forwards for all addons (best-effort background).
	for name, addon := range addons {
		ns := addon.Namespace
		if ns == "" {
			ns = "default"
		}
		for portStr, target := range addon.PortForward {
			var localPort uint16
			fmt.Sscanf(portStr, "%d", &localPort)
			if localPort == 0 {
				continue
			}
			go maintainPortForward(ctx, cs.KubeconfigPath, ns, target, localPort)
		}
		_ = name
	}
	return nil
}

func installAddon(ctx context.Context, name string, addon *config.AddonConfig, cs *state.ClusterState) error {
	switch addon.Type {
	case "helm":
		return installHelm(ctx, name, addon, cs)
	case "manifest":
		return installManifest(ctx, addon, cs)
	case "kustomize":
		return KubectlApplyDir(ctx, cs.KubeconfigPath, addon.Path, true)
	default:
		return fmt.Errorf("unknown addon type %q", addon.Type)
	}
}

func installHelm(ctx context.Context, name string, addon *config.AddonConfig, cs *state.ClusterState) error {
	ns := addon.Namespace
	if ns == "" {
		ns = "default"
	}

	// Add repo if needed.
	if addon.Repo != nil && !strings.HasPrefix(addon.Chart, "oci://") {
		repoName := strings.ReplaceAll(name, "/", "-") + "-repo"
		repoArgs := []string{"repo", "add", repoName, *addon.Repo}
		helm(ctx, cs.KubeconfigPath, repoArgs...)
		helm(ctx, cs.KubeconfigPath, "repo", "update")
	}

	args := []string{"upgrade", "--install", name, addon.Chart,
		"--namespace", ns, "--create-namespace",
	}
	if addon.Version != nil {
		args = append(args, "--version", *addon.Version)
	}
	if addon.Wait {
		args = append(args, "--wait", "--timeout", addon.Timeout)
	}
	if !addon.SkipCRDs {
		args = append(args, "--include-crds")
	}

	for _, vf := range addon.ValuesFiles {
		args = append(args, "--values", vf)
	}

	if len(addon.Values) > 0 {
		valData, err := yaml.Marshal(addon.Values)
		if err == nil {
			tmp, err := os.CreateTemp("", "devrig-values-*.yaml")
			if err == nil {
				_, _ = tmp.Write(valData)
				tmp.Close()
				args = append(args, "--values", tmp.Name())
				defer os.Remove(tmp.Name())
			}
		}
	}

	if err := helm(ctx, cs.KubeconfigPath, args...); err != nil {
		// Retry once on CRD-mapping errors.
		if strings.Contains(err.Error(), "resource mapping not found") {
			time.Sleep(3 * time.Second)
			return helm(ctx, cs.KubeconfigPath, args...)
		}
		return err
	}
	return nil
}

func installManifest(ctx context.Context, addon *config.AddonConfig, cs *state.ClusterState) error {
	if addon.Path == "" {
		return nil
	}
	// Poll for up to 5 minutes on CRD-not-found errors.
	return pollWithBackoff(ctx, 5*time.Minute, 3*time.Second, 30*time.Second, func() error {
		return KubectlApplyDir(ctx, cs.KubeconfigPath, addon.Path, false)
	})
}

func helm(ctx context.Context, kubeconfig string, args ...string) error {
	cmd := exec.CommandContext(ctx, "helm", args...)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("helm %s: %w\n%s", args[0], err, strings.TrimSpace(string(out)))
	}
	return nil
}

// maintainPortForward restarts a port-forward whenever it exits, with
// exponential backoff. It stops when ctx is cancelled.
func maintainPortForward(ctx context.Context, kubeconfig, namespace, target string, localPort uint16) {
	// Split target: "service/name:port" or "pod/name:port"
	colon := strings.LastIndex(target, ":")
	resource := target
	remotePort := ""
	if colon != -1 {
		resource = target[:colon]
		remotePort = target[colon+1:]
	}

	delay := time.Second
	stableAt := time.Time{}
	for {
		if ctx.Err() != nil {
			return
		}
		pfArgs := []string{"port-forward", "-n", namespace, resource}
		if remotePort != "" {
			pfArgs = append(pfArgs, fmt.Sprintf("%d:%s", localPort, remotePort))
		} else {
			pfArgs = append(pfArgs, fmt.Sprintf("%d", localPort))
		}
		cmd := exec.CommandContext(ctx, "kubectl", pfArgs...)
		cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
		start := time.Now()
		_ = cmd.Run()
		if ctx.Err() != nil {
			return
		}
		// If we ran for >60s, reset the backoff.
		if time.Since(start) > 60*time.Second {
			delay = time.Second
			stableAt = time.Time{}
		}
		if stableAt.IsZero() {
			stableAt = time.Now()
		}
		select {
		case <-ctx.Done():
			return
		case <-time.After(delay):
		}
		delay *= 2
		if delay > 30*time.Second {
			delay = 30 * time.Second
		}
	}
}

// addonTopoSort returns addon names in dependency order (Kahn's algorithm).
func addonTopoSort(addons map[string]config.AddonConfig) ([]string, error) {
	inDegree := make(map[string]int)
	deps := make(map[string][]string) // name → names that must come before
	for name, a := range addons {
		inDegree[name] = inDegree[name] // ensure entry exists
		for _, dep := range a.DependsOn {
			deps[dep] = append(deps[dep], name)
			inDegree[name]++
		}
	}

	var queue []string
	for name := range addons {
		if inDegree[name] == 0 {
			queue = append(queue, name)
		}
	}
	sort.Strings(queue)

	var order []string
	for len(queue) > 0 {
		n := queue[0]
		queue = queue[1:]
		order = append(order, n)
		next := deps[n]
		sort.Strings(next)
		for _, m := range next {
			inDegree[m]--
			if inDegree[m] == 0 {
				queue = append(queue, m)
			}
		}
	}
	if len(order) != len(addons) {
		return nil, fmt.Errorf("addon dependency cycle detected")
	}
	return order, nil
}

// addonManifestPath returns an absolute path, resolving relative paths against
// the project config directory.
func addonManifestPath(path, configDir string) string {
	if filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(configDir, path)
}
