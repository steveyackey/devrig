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
	"github.com/steveyackey/devrig/internal/verbose"
)

// Manager orchestrates the full cluster lifecycle.
type Manager struct {
	cfg       *config.ClusterConfig
	slug      string
	stateDir  string
	configDir string
	network   string
	tools     *tools.Resolver
}

// NewManager creates a cluster manager. slug and network are the devrig
// project slug and Docker network name. configDir is the directory containing
// the devrig config file, used to resolve relative volume host paths. resolver
// locates the k3d/kubectl/helm binaries; pass nil for defaults (managed,
// on-demand fetch disabled).
func NewManager(cfg *config.ClusterConfig, resolver *tools.Resolver, slug, stateDir, configDir, network string) *Manager {
	if resolver == nil {
		resolver = tools.NewResolver(tools.Options{})
	}
	return &Manager{cfg: cfg, slug: slug, stateDir: stateDir, configDir: configDir, network: network, tools: resolver}
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

	curVer := m.k3dVersion(ctx)

	switch {
	case !exists:
		if err := m.create(ctx, &cs); err != nil {
			return nil, err
		}
	case m.k3dVersionSkewed(curVer):
		// The k3d that created this cluster differs from the one reusing it now
		// (e.g. a switch between system and vendored k3d). The serverlb's config
		// layout can be incompatible across versions, so `cluster start` would
		// fail with an opaque "/etc/confd/values.yaml not found". Recreate up
		// front instead of waiting for that failure.
		if err := m.recreate(ctx, &cs); err != nil {
			return nil, err
		}
	default:
		if err := m.reuse(ctx, &cs); err != nil {
			return nil, err
		}
	}

	// Stamp the k3d version that now owns the cluster so the next run can detect
	// skew. Preserve any prior stamp if we couldn't read the current version.
	if curVer != "" {
		cs.K3dVersion = curVer
	} else {
		cs.K3dVersion = recordedK3dVersion(m.stateDir)
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
		// Bind the API server to loopback with a random free port. Without this
		// k3d binds the serverlb on 0.0.0.0, and the kubeconfig it emits then
		// carries a server address the host can't reliably dial — notably on
		// Windows/Docker Desktop, where kubectl hangs trying to reach 0.0.0.0.
		// ":0" picks a free port; writeKubeconfig resolves it back to 127.0.0.1.
		"--api-port", "127.0.0.1:0",
		// Leave the user's ~/.kube/config and current context untouched; devrig
		// drives the cluster through its own kubeconfig in the state dir.
		"--kubeconfig-update-default=false",
		"--kubeconfig-switch-context=false",
	}

	for _, p := range m.cfg.Ports {
		args = append(args, "--port", p)
	}
	for _, v := range m.cfg.Volumes {
		args = append(args, "--volume", resolveClusterVolume(v, m.configDir))
	}
	for _, a := range m.cfg.K3SArgs {
		// Modern k3d requires a node filter on --k3s-arg; scope each to the
		// server nodes (omitting the filter makes `cluster create` error out).
		args = append(args, "--k3s-arg", a+"@server:*")
	}

	if m.cfg.Registry {
		regName := m.RegistryName()
		args = append(args, "--registry-create", regName+":0.0.0.0:0")
		cs.RegistryName = &regName
	}

	// External registry mirrors/auth: write registries.yaml and point k3d at it.
	if len(m.cfg.Registries) > 0 {
		if err := m.ApplyRegistriesConfig(ctx, m.stateDir); err != nil {
			return fmt.Errorf("cluster: registries config: %w", err)
		}
		args = append(args, "--registry-config", filepath.Join(m.stateDir, "registries.yaml"))
	}

	if out, err := m.k3d(ctx, args...); err != nil {
		// The cluster may already exist — left over from a prior run, or the
		// up-front existence check returned a false negative (e.g. a transient
		// `k3d cluster list` failure). Either way, reuse it instead of failing
		// with a confusing "already exists" error; that's the same outcome as
		// the exists path in Ensure.
		if clusterAlreadyExists(out) {
			return m.reuse(ctx, cs)
		}
		return fmt.Errorf("cluster create: %w\n%s", err, out)
	}

	if err := m.writeKubeconfig(ctx, name); err != nil {
		return err
	}

	return nil
}

// reuse adopts an already-existing cluster: it makes sure the nodes are running
// (k3d cluster start is idempotent — a no-op when they already are) and refreshes
// the kubeconfig. Used both when Ensure detects an existing cluster and when
// create races/falls back onto one.
func (m *Manager) reuse(ctx context.Context, cs *state.ClusterState) error {
	name := m.ClusterName()
	out, err := m.k3d(ctx, "cluster", "start", name)
	if err == nil {
		return m.writeKubeconfig(ctx, name)
	}
	// A leftover cluster whose serverlb (load balancer) won't come ready. The
	// usual cause is k3d version skew — the cluster was created by a different
	// k3d than the one reusing it now (e.g. a system k3d created it before a
	// switch to vendored deps), so the serverlb's confd config isn't where this
	// k3d expects it ("/etc/confd/values.yaml ... file not found"). A Docker
	// restart or an interrupted create produce the same symptom. `cluster start`
	// can't regenerate that config — only a fresh create can — so tear the
	// cluster down and recreate it instead of surfacing an opaque k3d failure.
	if serverlbStartBroken(out) {
		return m.recreate(ctx, cs)
	}
	return fmt.Errorf("cluster start: %w\n%s", err, out)
}

// recreate tears a broken cluster down and creates it fresh. The cluster's data
// is discarded — acceptable for devrig's ephemeral dev clusters, and the same
// thing a user would do by hand to clear a wedged serverlb.
func (m *Manager) recreate(ctx context.Context, cs *state.ClusterState) error {
	name := m.ClusterName()
	if out, err := m.k3d(ctx, "cluster", "delete", name); err != nil {
		return fmt.Errorf("cluster recreate: delete %s: %w\n%s", name, err, out)
	}
	if m.cfg.Registry {
		// The registry is created alongside the cluster; drop it so create can
		// re-add it cleanly (best-effort — it may already be gone).
		_, _ = m.k3d(ctx, "registry", "delete", m.RegistryName())
	}
	return m.create(ctx, cs)
}

// k3dVersion returns the version of the resolved k3d binary (e.g. "v5.9.0"),
// or "" if it can't be determined.
func (m *Manager) k3dVersion(ctx context.Context) string {
	bin, err := m.tools.Path(ctx, tools.K3d)
	if err != nil {
		return ""
	}
	out, _ := verbose.Run(exec.CommandContext(ctx, bin, "version"))
	return parseK3dVersion(out)
}

// parseK3dVersion extracts the version token from `k3d version` output, whose
// first line reads e.g. "k3d version v5.9.0" (a second line reports k3s).
func parseK3dVersion(out string) string {
	for _, line := range strings.Split(out, "\n") {
		f := strings.Fields(line)
		if len(f) >= 3 && f[0] == "k3d" && f[1] == "version" {
			return f[2]
		}
	}
	return ""
}

// recordedK3dVersion reads the k3d version stamped on the persisted cluster
// state, or "" if there is none.
func recordedK3dVersion(stateDir string) string {
	if prev := state.Load(stateDir); prev != nil && prev.Cluster != nil {
		return prev.Cluster.K3dVersion
	}
	return ""
}

// k3dVersionSkewed reports whether the current k3d version differs from the one
// recorded as having created the cluster. A missing record or unknown current
// version is treated as "not skewed" — reuse (and its own recovery) handles it.
func (m *Manager) k3dVersionSkewed(curVer string) bool {
	prev := recordedK3dVersion(m.stateDir)
	return prev != "" && curVer != "" && prev != curVer
}

// serverlbStartBroken reports whether `k3d cluster start` failed because the
// serverlb (load balancer) helper node could not become ready — the signature
// of a stale serverlb container that only a fresh create can fix.
func serverlbStartBroken(out string) bool {
	l := strings.ToLower(out)
	return strings.Contains(l, "failed to get ready") ||
		strings.Contains(l, "failed to add one or more helper nodes") ||
		(strings.Contains(l, "loadbalancer config") && strings.Contains(l, "values.yaml"))
}

// clusterAlreadyExists reports whether k3d output indicates the cluster could not
// be created because one with that name already exists.
func clusterAlreadyExists(out string) bool {
	return strings.Contains(strings.ToLower(out), "already exists")
}

// resolveClusterVolume resolves a relative host source path in a k3d volume
// spec to an absolute path, anchored at configDir. k3d volume specs are
// "SOURCE:DEST[@NODEFILTER]" (e.g. "../:/workspace@server:*"). Docker rejects a
// relative SOURCE ("../" => "invalid characters for a local volume name"), so a
// relative host path must be made absolute. Named volumes and already-absolute
// paths (including Windows drive paths like "C:\...") are returned unchanged.
func resolveClusterVolume(spec, configDir string) string {
	// Split off the optional "@nodefilter" suffix; its own ":" (e.g.
	// "server:*") must not be confused with the SOURCE:DEST separator.
	volPart, suffix := spec, ""
	if at := strings.Index(spec, "@"); at != -1 {
		volPart, suffix = spec[:at], spec[at:]
	}

	src, dest, ok := splitVolumeSpec(volPart)
	if !ok {
		return spec // no DEST separator — leave untouched
	}
	// Only relative host paths need resolving. Absolute paths (incl. Windows
	// "C:\...") and bare named volumes ("pgdata") are passed through.
	if src == "" || filepath.IsAbs(src) {
		return spec
	}
	if !strings.HasPrefix(src, ".") && !strings.ContainsAny(src, `/\`) {
		return spec // named volume, not a host path
	}

	abs, err := filepath.Abs(filepath.Join(configDir, src))
	if err != nil {
		return spec
	}
	return abs + ":" + dest + suffix
}

// splitVolumeSpec splits "SOURCE:DEST" into its parts, tolerating a Windows
// drive-letter colon in SOURCE (e.g. "C:\path:/dest").
func splitVolumeSpec(s string) (src, dest string, ok bool) {
	start := 0
	if len(s) >= 2 && s[1] == ':' &&
		((s[0] >= 'a' && s[0] <= 'z') || (s[0] >= 'A' && s[0] <= 'Z')) {
		start = 2 // skip the drive-letter colon
	}
	i := strings.Index(s[start:], ":")
	if i == -1 {
		return "", "", false
	}
	i += start
	return s[:i], s[i+1:], true
}

// writeKubeconfig extracts the kubeconfig from k3d to the state dir.
func (m *Manager) writeKubeconfig(ctx context.Context, name string) error {
	if err := os.MkdirAll(m.stateDir, 0o755); err != nil {
		return fmt.Errorf("cluster: create state dir: %w", err)
	}
	// Capture stdout only: k3d logs non-fatal warnings (e.g. "error getting
	// loadbalancer config" while the serverlb is still initializing) to stderr,
	// and merging them into the file would corrupt the kubeconfig YAML.
	//
	// Deliberately omit k3d's --verbose flag here: k3d writes its debug log to
	// stdout (not stderr), which is the same stream the kubeconfig is printed
	// on, so --verbose would interleave control-character log lines into the
	// kubeconfig YAML. The stdout/stderr split RunSplit relies on can't protect
	// against output that k3d itself puts on stdout.
	bin, err := m.tools.Path(ctx, tools.K3d)
	if err != nil {
		return err
	}
	stdout, stderr, err := verbose.RunSplit(exec.CommandContext(ctx, bin, "kubeconfig", "get", name))
	if err != nil {
		return fmt.Errorf("cluster: get kubeconfig: %w\n%s", err, stderr)
	}
	if err := os.WriteFile(m.KubeconfigPath(), []byte(stdout+"\n"), 0o600); err != nil {
		return fmt.Errorf("cluster: write kubeconfig: %w", err)
	}
	return m.fixKubeconfigServer(ctx, name)
}

// fixKubeconfigServer normalizes the API server address in the written
// kubeconfig so the host can reach it. With `--api-port 127.0.0.1:0`, k3d
// sometimes leaves the server URL with an unresolved ":0" port (and may emit a
// 0.0.0.0 host); both are unreachable from the host — especially on Windows.
// We discover the serverlb's published 6443 port from Docker and rewrite the
// server to https://127.0.0.1:<port>.
func (m *Manager) fixKubeconfigServer(ctx context.Context, name string) error {
	content, err := os.ReadFile(m.KubeconfigPath())
	if err != nil {
		return fmt.Errorf("cluster: read kubeconfig for fix: %w", err)
	}
	if !kubeconfigNeedsServerFix(string(content)) {
		return nil
	}

	port, err := serverlbAPIPort(ctx, name)
	if err != nil {
		return err
	}
	fixed := rewriteKubeconfigServer(string(content), port)
	if err := os.WriteFile(m.KubeconfigPath(), []byte(fixed), 0o600); err != nil {
		return fmt.Errorf("cluster: write fixed kubeconfig: %w", err)
	}
	return nil
}

// kubeconfigNeedsServerFix reports whether the kubeconfig has a server line with
// an unresolved ":0" port or a non-dialable 0.0.0.0 host that must be rewritten.
func kubeconfigNeedsServerFix(content string) bool {
	for _, line := range strings.Split(content, "\n") {
		t := strings.TrimSpace(line)
		if strings.HasPrefix(t, "server:") && (strings.HasSuffix(t, ":0") || strings.Contains(t, "0.0.0.0")) {
			return true
		}
	}
	return false
}

// rewriteKubeconfigServer points the API server at https://127.0.0.1:<port>,
// replacing both unresolved ":0" forms and a 0.0.0.0 host on the real port.
func rewriteKubeconfigServer(content, port string) string {
	server := "https://127.0.0.1:" + port
	for _, old := range []string{"https://127.0.0.1:0", "https://0.0.0.0:0", "https://0.0.0.0:" + port} {
		content = strings.ReplaceAll(content, old, server)
	}
	return content
}

// serverlbAPIPort returns the host port the k3d serverlb publishes for the
// API server (container port 6443/tcp).
func serverlbAPIPort(ctx context.Context, clusterName string) (string, error) {
	container := fmt.Sprintf("k3d-%s-serverlb", clusterName)
	cmd := exec.CommandContext(ctx, "docker", "inspect", container,
		"--format", `{{(index .NetworkSettings.Ports "6443/tcp" 0).HostPort}}`)
	port, stderr, err := verbose.RunSplit(cmd)
	if err != nil {
		return "", fmt.Errorf("cluster: inspect %s for API port: %w\n%s", container, err, stderr)
	}
	if port == "" || port == "0" {
		return "", fmt.Errorf("cluster: could not resolve API server port from %s (got %q)", container, port)
	}
	return port, nil
}

// ListDevrigClusters returns the names of all k3d clusters devrig manages
// (those named "devrig-*"), discovered via `k3d cluster list` regardless of any
// local state or registry entry. It exists so `devrig delete --all` can reap a
// cluster orphaned by a `devrig start` that was interrupted (e.g. Ctrl-C'd while
// hanging in cluster creation) before it ever recorded the instance — the
// registry-driven cleanup path can't see such a cluster, but k3d still can.
func ListDevrigClusters(ctx context.Context, r *tools.Resolver) ([]string, error) {
	bin, err := r.Path(ctx, tools.K3d)
	if err != nil {
		return nil, err
	}
	// Capture stdout only — k3d logs warnings to stderr that would corrupt the
	// JSON (same reason as clusterExists/writeKubeconfig).
	stdout, stderr, err := verbose.RunSplit(exec.CommandContext(ctx, bin, "cluster", "list", "-o", "json"))
	if err != nil {
		return nil, fmt.Errorf("k3d cluster list: %w\n%s", err, stderr)
	}
	return parseDevrigClusterNames(stdout)
}

// parseDevrigClusterNames extracts the names of devrig-managed clusters from
// `k3d cluster list -o json` output.
func parseDevrigClusterNames(out string) ([]string, error) {
	var clusters []struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal([]byte(extractJSON(out)), &clusters); err != nil {
		return nil, fmt.Errorf("parse k3d cluster list: %w", err)
	}
	var names []string
	for _, c := range clusters {
		if strings.HasPrefix(c.Name, "devrig-") {
			names = append(names, c.Name)
		}
	}
	return names, nil
}

// DeleteClusterByName deletes a k3d cluster and its paired registry by cluster
// name alone (the registry deletion is best-effort). Unlike Manager.Delete it
// needs no config or slug, so it can tear down an orphaned cluster discovered
// via ListDevrigClusters. The registry created alongside a "devrig-<slug>"
// cluster is named "k3d-devrig-<slug>-reg" — i.e. "k3d-<clustername>-reg".
func DeleteClusterByName(ctx context.Context, r *tools.Resolver, name string) error {
	bin, err := r.Path(ctx, tools.K3d)
	if err != nil {
		return err
	}
	delArgs := append([]string{"cluster", "delete", name}, tools.VerboseFlags(tools.K3d)...)
	if out, err := runCmd(ctx, bin, delArgs...); err != nil {
		return fmt.Errorf("cluster delete %s: %w\n%s", name, err, out)
	}
	_, _ = runCmd(ctx, bin, "registry", "delete", "k3d-"+name+"-reg") // best-effort
	return nil
}

// clusterExists checks via `k3d cluster list -o json` whether the named cluster
// exists (running or stopped).
func (m *Manager) clusterExists(ctx context.Context, name string) (bool, error) {
	// Capture stdout only: k3d logs non-fatal warnings to stderr, and merged
	// into the output they corrupt the JSON so the parse below silently fails —
	// the cluster then looks absent and create() hits "already exists". (Same
	// reason writeKubeconfig reads stdout only.)
	out, err := m.k3dStdout(ctx, "cluster", "list", "-o", "json")
	if err != nil {
		return false, nil // treat error (k3d not found, etc.) as "doesn't exist"
	}
	var clusters []struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal([]byte(extractJSON(out)), &clusters); err != nil {
		return false, nil
	}
	for _, c := range clusters {
		if c.Name == name {
			return true, nil
		}
	}
	return false, nil
}

// extractJSON returns out starting at the first line that begins with a JSON
// array/object delimiter, so a leading k3d log line (e.g. "WARN[0000] ...", which
// itself contains a '[') doesn't break parsing if one reaches stdout.
func extractJSON(out string) string {
	lines := strings.Split(out, "\n")
	for i, line := range lines {
		if t := strings.TrimSpace(line); strings.HasPrefix(t, "[") || strings.HasPrefix(t, "{") {
			return strings.Join(lines[i:], "\n")
		}
	}
	return out
}

// k3dStdout runs k3d and returns trimmed stdout only (stderr is captured for
// errors, and streamed live in verbose mode), for commands whose stdout must be
// parsed (e.g. JSON listings).
func (m *Manager) k3dStdout(ctx context.Context, args ...string) (string, error) {
	bin, err := m.tools.Path(ctx, tools.K3d)
	if err != nil {
		return "", err
	}
	// Deliberately omit k3d's --verbose flag: k3d writes its debug log to stdout,
	// so --verbose would interleave control-character log lines into the JSON we
	// parse here (see writeKubeconfig for the same reason).
	stdout, stderr, err := verbose.RunSplit(exec.CommandContext(ctx, bin, args...))
	if err != nil {
		return "", fmt.Errorf("k3d %s: %w\n%s", strings.Join(args, " "), err, stderr)
	}
	return stdout, nil
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

// runCmd runs a command and returns combined output (streamed live in verbose mode).
func runCmd(ctx context.Context, name string, args ...string) (string, error) {
	return verbose.Run(exec.CommandContext(ctx, name, args...))
}

// k3d resolves the k3d binary (managed or system) and runs it.
func (m *Manager) k3d(ctx context.Context, args ...string) (string, error) {
	bin, err := m.tools.Path(ctx, tools.K3d)
	if err != nil {
		return "", err
	}
	args = append(args, tools.VerboseFlags(tools.K3d)...)
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
	args := append([]string{"apply", flag, path}, tools.VerboseFlags(tools.Kubectl)...)
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	if out, err := verbose.Run(cmd); err != nil {
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
	args := append([]string{"rollout", "restart",
		fmt.Sprintf("deployment/%s", name), "-n", namespace}, tools.VerboseFlags(tools.Kubectl)...)
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
	if out, err := verbose.Run(cmd); err != nil {
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
