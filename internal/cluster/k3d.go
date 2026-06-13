// Package cluster manages the k3d cluster lifecycle for devrig.
package cluster

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/state"
	"github.com/steveyackey/devrig/internal/tools"
)

// Manager orchestrates the full cluster lifecycle.
type Manager struct {
	cfg      *config.ClusterConfig
	slug     string
	stateDir string
	network  string
	tools    *tools.Resolver
}

// NewManager creates a cluster manager. slug and network are the devrig
// project slug and Docker network name. resolver locates the k3d/kubectl/helm
// binaries; pass nil for defaults (managed, on-demand fetch disabled).
func NewManager(cfg *config.ClusterConfig, resolver *tools.Resolver, slug, stateDir, network string) *Manager {
	if resolver == nil {
		resolver = tools.NewResolver(tools.Options{})
	}
	return &Manager{cfg: cfg, slug: slug, stateDir: stateDir, network: network, tools: resolver}
}

// ClusterName returns the k3d cluster name (dev-friendly if not configured).
func (m *Manager) ClusterName() string {
	if m.cfg.Name != nil && *m.cfg.Name != "" {
		return *m.cfg.Name
	}
	return "devrig-" + m.slug
}

// KubeconfigPath returns the path where the kubeconfig is written.
func (m *Manager) KubeconfigPath() string {
	return filepath.Join(m.stateDir, "kubeconfig")
}

// RegistryName returns the k3d registry container name.
func (m *Manager) RegistryName() string {
	return "k3d-devrig-" + m.slug + "-reg"
}

// Ensure creates the cluster if it doesn't exist, then returns the
// cluster state (kubeconfig path, registry info).
func (m *Manager) Ensure(ctx context.Context) (*state.ClusterState, error) {
	name := m.ClusterName()

	// Check if cluster already exists.
	exists, err := m.clusterExists(ctx, name)
	if err != nil {
		return nil, err
	}

	var cs state.ClusterState
	cs.ClusterName = name
	cs.KubeconfigPath = m.KubeconfigPath()
	cs.DeployedServices = make(map[string]state.ClusterDeployState)
	cs.InstalledAddons = make(map[string]state.AddonState)

	if !exists {
		if err := m.create(ctx, &cs); err != nil {
			return nil, err
		}
	} else {
		if err := m.writeKubeconfig(ctx, name); err != nil {
			return nil, err
		}
	}

	if m.cfg.Registry {
		rPort, err := RegistryHostPort(m.RegistryName())
		if err != nil {
			return nil, fmt.Errorf("cluster: registry host port: %w", err)
		}
		regName := m.RegistryName()
		cs.RegistryName = &regName
		cs.RegistryPort = &rPort
	}

	return &cs, nil
}

// Delete removes the k3d cluster and (optionally) the registry.
func (m *Manager) Delete(ctx context.Context) error {
	name := m.ClusterName()
	args := []string{"cluster", "delete", name}
	if out, err := m.k3d(ctx, args...); err != nil {
		return fmt.Errorf("cluster delete: %w\n%s", err, out)
	}
	if m.cfg.Registry {
		regArgs := []string{"registry", "delete", m.RegistryName()}
		_, _ = m.k3d(ctx, regArgs...) // best-effort
	}
	return nil
}

// create runs k3d cluster create with all configured options.
func (m *Manager) create(ctx context.Context, cs *state.ClusterState) error {
	name := m.ClusterName()
	args := []string{"cluster", "create", name,
		"--network", m.network,
		"--agents", fmt.Sprintf("%d", m.cfg.Agents),
	}

	for _, p := range m.cfg.Ports {
		args = append(args, "--port", p)
	}
	for _, v := range m.cfg.Volumes {
		args = append(args, "--volume", v)
	}
	for _, a := range m.cfg.K3SArgs {
		args = append(args, "--k3s-arg", a)
	}

	if m.cfg.Registry {
		regName := m.RegistryName()
		args = append(args, "--registry-create", regName+":0.0.0.0:0")
		cs.RegistryName = &regName
	}

	if out, err := m.k3d(ctx, args...); err != nil {
		return fmt.Errorf("cluster create: %w\n%s", err, out)
	}

	if err := m.writeKubeconfig(ctx, name); err != nil {
		return err
	}

	return nil
}

// writeKubeconfig extracts the kubeconfig from k3d to the state dir.
func (m *Manager) writeKubeconfig(ctx context.Context, name string) error {
	if err := os.MkdirAll(m.stateDir, 0o755); err != nil {
		return fmt.Errorf("cluster: create state dir: %w", err)
	}
	out, err := m.k3d(ctx, "kubeconfig", "get", name)
	if err != nil {
		return fmt.Errorf("cluster: get kubeconfig: %w\n%s", err, out)
	}
	if err := os.WriteFile(m.KubeconfigPath(), []byte(out), 0o600); err != nil {
		return fmt.Errorf("cluster: write kubeconfig: %w", err)
	}
	return nil
}

// clusterExists checks via `k3d cluster list -o json` whether the named cluster is running.
func (m *Manager) clusterExists(ctx context.Context, name string) (bool, error) {
	out, err := m.k3d(ctx, "cluster", "list", "-o", "json")
	if err != nil {
		return false, nil // treat error (k3d not found, etc.) as "doesn't exist"
	}
	var clusters []struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal([]byte(out), &clusters); err != nil {
		return false, nil
	}
	for _, c := range clusters {
		if c.Name == name {
			return true, nil
		}
	}
	return false, nil
}

// ApplyRegistriesConfig writes a registries.yaml to the k3d data dir if
// external registries are configured, for mirror/auth support.
func (m *Manager) ApplyRegistriesConfig(ctx context.Context, stateDir string) error {
	if len(m.cfg.Registries) == 0 {
		return nil
	}
	// Build a minimal k3d registries.yaml structure.
	var sb strings.Builder
	sb.WriteString("configs:\n")
	for _, r := range m.cfg.Registries {
		sb.WriteString(fmt.Sprintf("  %q:\n", r.URL))
		if r.Username != "" {
			sb.WriteString(fmt.Sprintf("    auth:\n      username: %q\n      password: %q\n", r.Username, r.Password))
		}
	}
	path := filepath.Join(stateDir, "registries.yaml")
	return os.WriteFile(path, []byte(sb.String()), 0o600)
}

// runCmd runs a command and returns combined output.
func runCmd(ctx context.Context, name string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	out, err := cmd.CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

// k3d resolves the k3d binary (managed or system) and runs it.
func (m *Manager) k3d(ctx context.Context, args ...string) (string, error) {
	bin, err := m.tools.Path(ctx, tools.K3d)
	if err != nil {
		return "", err
	}
	return runCmd(ctx, bin, args...)
}

// KubectlApply runs kubectl apply -f path with the given kubeconfig.
func KubectlApply(ctx context.Context, r *tools.Resolver, kubeconfig, path string) error {
	return KubectlApplyDir(ctx, r, kubeconfig, path, false)
}

// KubectlApplyDir runs kubectl apply -f (or -k for kustomize) on a path.
func KubectlApplyDir(ctx context.Context, r *tools.Resolver, kubeconfig, path string, kustomize bool) error {
	bin, err := r.Path(ctx, tools.Kubectl)
	if err != nil {
		return err
	}
	flag := "-f"
	if kustomize {
		flag = "-k"
	}
	cmd := exec.CommandContext(ctx, bin, "apply", flag, path)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("kubectl apply: %w\n%s", err, out)
	}
	return nil
}

// KubectlRollout restarts a deployment by name in the given namespace.
func KubectlRollout(ctx context.Context, r *tools.Resolver, kubeconfig, namespace, name string) error {
	bin, err := r.Path(ctx, tools.Kubectl)
	if err != nil {
		return err
	}
	cmd := exec.CommandContext(ctx, bin, "rollout", "restart",
		fmt.Sprintf("deployment/%s", name), "-n", namespace)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("kubectl rollout restart: %w\n%s", err, out)
	}
	return nil
}

// KubectlPortForward starts a port-forward subprocess in the background.
// The caller is responsible for cancelling ctx to stop the forward.
func KubectlPortForward(ctx context.Context, r *tools.Resolver, kubeconfig, namespace, target string, localPort uint16) {
	bin, err := r.Path(ctx, tools.Kubectl)
	if err != nil {
		return
	}
	cmd := exec.CommandContext(ctx, bin, "port-forward",
		"-n", namespace, target,
		fmt.Sprintf("%d:%s", localPort, target[strings.LastIndex(target, ":")+1:]),
	)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	_ = cmd.Start()
	go func() {
		_ = cmd.Wait()
	}()
}

// pollWithBackoff retries fn until it succeeds or ctx expires, using exponential
// backoff starting at minDelay, capped at maxDelay, for at most timeout.
func pollWithBackoff(ctx context.Context, timeout, minDelay, maxDelay time.Duration, fn func() error) error {
	deadline := time.Now().Add(timeout)
	delay := minDelay
	for {
		if err := fn(); err == nil {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("timed out after %s", timeout)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(delay):
		}
		delay *= 2
		if delay > maxDelay {
			delay = maxDelay
		}
	}
}
