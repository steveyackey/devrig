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
	"github.com/steveyackey/devrig/internal/tools"
	"github.com/steveyackey/devrig/internal/verbose"
)

// InstallAddons installs all configured addons in topological order,
// then starts port-forwards. ctx cancellation stops the port-forwards.
// r resolves the kubectl/helm binaries.
func InstallAddons(
	ctx context.Context,
	r *tools.Resolver,
	addons map[string]config.AddonConfig,
	cs *state.ClusterState,
	stateDir string,
	configDir string,
) error {
	order, err := addonTopoSort(addons)
	if err != nil {
		return fmt.Errorf("addon install: %w", err)
	}

	for _, name := range order {
		addon := addons[name]
		if err := installAddon(ctx, r, name, &addon, cs, configDir); err != nil {
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
			go maintainPortForward(ctx, r, cs.KubeconfigPath, ns, target, localPort)
		}
		_ = name
	}
	return nil
}

func installAddon(ctx context.Context, r *tools.Resolver, name string, addon *config.AddonConfig, cs *state.ClusterState, configDir string) error {
	switch addon.Type {
	case "helm":
		return installHelm(ctx, r, name, addon, cs, configDir)
	case "manifest":
		return installManifest(ctx, r, addon, cs, configDir)
	case "kustomize":
		return KubectlApplyDir(ctx, r, cs.KubeconfigPath, addonManifestPath(addon.Path, configDir), true)
	default:
		return fmt.Errorf("unknown addon type %q", addon.Type)
	}
}

// expandDottedKeys turns flat dotted keys (e.g. "image.tag") into nested maps
// so they apply when written to a helm --values file. Helm only splits dotted
// keys for --set, not --values, so without this an addon value like
// `"image.tag" = "..."` would land as a literal key the chart ignores.
func expandDottedKeys(m map[string]any) map[string]any {
	out := make(map[string]any, len(m))
	for k, v := range m {
		if !strings.Contains(k, ".") {
			out[k] = v
			continue
		}
		parts := strings.Split(k, ".")
		cur := out
		for i, p := range parts {
			if i == len(parts)-1 {
				cur[p] = v
				break
			}
			next, ok := cur[p].(map[string]any)
			if !ok {
				next = make(map[string]any)
				cur[p] = next
			}
			cur = next
		}
	}
	return out
}

func installHelm(ctx context.Context, r *tools.Resolver, name string, addon *config.AddonConfig, cs *state.ClusterState, configDir string) error {
	ns := addon.Namespace
	if ns == "" {
		ns = "default"
	}

	// Add repo if needed.
	if addon.Repo != nil && !strings.HasPrefix(addon.Chart, "oci://") {
		repoName := strings.ReplaceAll(name, "/", "-") + "-repo"
		repoArgs := []string{"repo", "add", repoName, *addon.Repo}
		helm(ctx, r, cs.KubeconfigPath, repoArgs...)
		helm(ctx, r, cs.KubeconfigPath, "repo", "update")
	}

	// Resolve a local chart path (no repo, not OCI) relative to the config dir.
	chart := addon.Chart
	if addon.Repo == nil && !strings.HasPrefix(chart, "oci://") &&
		(strings.HasPrefix(chart, ".") || strings.ContainsAny(chart, `/\`)) {
		chart = addonManifestPath(chart, configDir)
	}

	args := []string{"upgrade", "--install", name, chart,
		"--namespace", ns, "--create-namespace",
	}
	if addon.Version != nil {
		args = append(args, "--version", *addon.Version)
	}
	if addon.Wait {
		args = append(args, "--wait", "--timeout", addon.Timeout)
	}
	// helm install/upgrade installs CRDs from the chart's crds/ dir by default;
	// --skip-crds opts out. (There is no --include-crds flag on install/upgrade
	// — that one is helm-template-only.)
	if addon.SkipCRDs {
		args = append(args, "--skip-crds")
	}

	for _, vf := range addon.ValuesFiles {
		args = append(args, "--values", addonManifestPath(vf, configDir))
	}

	if len(addon.Values) > 0 {
		valData, err := yaml.Marshal(expandDottedKeys(addon.Values))
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

	if err := helm(ctx, r, cs.KubeconfigPath, args...); err != nil {
		// Retry once on CRD-mapping errors.
		if strings.Contains(err.Error(), "resource mapping not found") {
			time.Sleep(3 * time.Second)
			return helm(ctx, r, cs.KubeconfigPath, args...)
		}
		return err
	}
	return nil
}

func installManifest(ctx context.Context, r *tools.Resolver, addon *config.AddonConfig, cs *state.ClusterState, configDir string) error {
	if addon.Path == "" {
		return nil
	}
	path := addonManifestPath(addon.Path, configDir)
	// Poll for up to 5 minutes on CRD-not-found errors.
	return pollWithBackoff(ctx, 5*time.Minute, 3*time.Second, 30*time.Second, func() error {
		return KubectlApplyDir(ctx, r, cs.KubeconfigPath, path, false)
	})
}

func helm(ctx context.Context, r *tools.Resolver, kubeconfig string, args ...string) error {
	bin, err := r.Path(ctx, tools.Helm)
	if err != nil {
		return err
	}
	subcmd := args[0]
	args = append(args, tools.VerboseFlags(tools.Helm)...)
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	if out, err := verbose.Run(cmd); err != nil {
		return fmt.Errorf("helm %s: %w\n%s", subcmd, err, out)
	}
	return nil
}

// maintainPortForward restarts a port-forward whenever it exits, with
// exponential backoff. It stops when ctx is cancelled.
func maintainPortForward(ctx context.Context, r *tools.Resolver, kubeconfig, namespace, target string, localPort uint16) {
	bin, err := r.Path(ctx, tools.Kubectl)
	if err != nil {
		return
	}
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
		cmd := exec.CommandContext(ctx, bin, pfArgs...)
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
