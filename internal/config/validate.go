package config

import (
	"fmt"
	"strings"
	"time"
)

// ValidationError collects multiple validation problems from a single config.
type ValidationError struct {
	Errors []string
}

func (e *ValidationError) Error() string {
	return fmt.Sprintf("%d config error(s):\n  %s", len(e.Errors), strings.Join(e.Errors, "\n  "))
}

func (e *ValidationError) add(format string, args ...any) {
	e.Errors = append(e.Errors, fmt.Sprintf(format, args...))
}

// Validate checks the config for structural errors: missing dependencies,
// dependency cycles, port conflicts, empty required fields, invalid values.
func Validate(cfg *Config) error {
	ve := &ValidationError{}

	// Build a set of all resource names for dependency resolution.
	allResources := allResourceNames(cfg)

	// Validate service fields and dependencies.
	for name, svc := range cfg.Services {
		if strings.TrimSpace(svc.Command) == "" {
			ve.add("services.%s: command must not be empty", name)
		}
		for _, dep := range svc.DependsOn {
			if _, ok := allResources[dep]; !ok {
				ve.add("services.%s: unknown dependency %q", name, dep)
			}
		}
		if svc.Restart != nil {
			if err := validateRestartPolicy(svc.Restart.Policy); err != nil {
				ve.add("services.%s.restart: %v", name, err)
			}
		}
	}

	// Validate docker fields and dependencies.
	for name, d := range cfg.Docker {
		if strings.TrimSpace(d.Image) == "" {
			ve.add("docker.%s: image must not be empty", name)
		}
		for _, dep := range d.DependsOn {
			if _, ok := allResources[dep]; !ok {
				ve.add("docker.%s: unknown dependency %q", name, dep)
			}
		}
		if err := validateVolumes(d.Volumes); err != nil {
			ve.add("docker.%s: %v", name, err)
		}
		if d.RegistryAuth != nil {
			if d.RegistryAuth.Username == "" {
				ve.add("docker.%s.registry_auth: username must not be empty", name)
			}
			if d.RegistryAuth.Password == "" {
				ve.add("docker.%s.registry_auth: password must not be empty", name)
			}
		}
	}

	// Validate compose.
	if cfg.Compose != nil {
		if strings.TrimSpace(cfg.Compose.File) == "" {
			ve.add("compose.file must not be empty")
		}
	}

	// Validate cluster.
	if cfg.Cluster != nil {
		for name, img := range cfg.Cluster.Images {
			if strings.TrimSpace(img.Context) == "" {
				ve.add("cluster.image.%s: context must not be empty", name)
			}
			// cluster.image names must not conflict with cluster.deploy
			if _, conflict := cfg.Cluster.Deploy[name]; conflict {
				ve.add("cluster.image.%s: name conflicts with cluster.deploy.%s", name, name)
			}
		}
		for name, dep := range cfg.Cluster.Deploy {
			if strings.TrimSpace(dep.Context) == "" {
				ve.add("cluster.deploy.%s: context must not be empty", name)
			}
			if strings.TrimSpace(dep.Manifests) == "" {
				ve.add("cluster.deploy.%s: manifests must not be empty", name)
			}
		}
		for name, addon := range cfg.Cluster.Addons {
			if err := validateAddon(name, &addon); err != nil {
				ve.add("cluster.addons.%s: %v", name, err)
			}
			for _, dep := range addon.DependsOn {
				if _, ok := cfg.Cluster.Addons[dep]; !ok {
					ve.add("cluster.addons.%s: depends_on %q must reference another addon", name, dep)
				}
			}
		}
		for i, reg := range cfg.Cluster.Registries {
			if reg.URL == "" {
				ve.add("cluster.registries[%d].url must not be empty", i)
			}
			if reg.Username == "" {
				ve.add("cluster.registries[%d].username must not be empty", i)
			}
			if reg.Password == "" {
				ve.add("cluster.registries[%d].password must not be empty", i)
			}
		}
		if cfg.Cluster.Logs != nil && cfg.Cluster.Logs.Enabled {
			if len(cfg.Cluster.Logs.ExcludeNamespaces) > 0 && !cfg.Cluster.Logs.Namespaces.All {
				ve.add("cluster.logs.exclude_namespaces requires namespaces = \"all\"")
			}
		}
		// Addon cycle detection
		if err := detectAddonCycles(cfg.Cluster.Addons); err != nil {
			ve.add("cluster.addons: %v", err)
		}
	}

	// Dashboard validation.
	if cfg.Dashboard != nil && cfg.Dashboard.OTel != nil {
		if err := validateRetention(cfg.Dashboard.OTel.Retention); err != nil {
			ve.add("dashboard.otel.retention: %v", err)
		}
		if cfg.Cluster != nil && cfg.Cluster.Logs != nil && cfg.Cluster.Logs.Enabled && cfg.Cluster.Logs.Collector {
			// cluster.logs.collector requires dashboard to receive OTLP
			// This is fine — dashboard OTel is present.
		}
	}
	if cfg.Cluster != nil && cfg.Cluster.Logs != nil && cfg.Cluster.Logs.Enabled && cfg.Cluster.Logs.Collector {
		if cfg.Dashboard == nil {
			ve.add("cluster.logs: cluster log collection requires [dashboard] to be configured (OTLP receiver)")
		}
	}

	// Port conflict detection across fixed ports.
	if portErrs := detectPortConflicts(cfg); len(portErrs) > 0 {
		for _, e := range portErrs {
			ve.add("%s", e)
		}
	}

	// Dependency cycle detection across all resources.
	if err := detectResourceCycles(cfg); err != nil {
		ve.add("dependency cycle: %v", err)
	}

	if len(ve.Errors) > 0 {
		return ve
	}
	return nil
}

func allResourceNames(cfg *Config) map[string]string {
	m := make(map[string]string)
	for name := range cfg.Services {
		m[name] = "service"
	}
	for name := range cfg.Docker {
		m[name] = "docker"
	}
	if cfg.Cluster != nil {
		for name := range cfg.Cluster.Images {
			m[name] = "cluster.image"
		}
		for name := range cfg.Cluster.Deploy {
			m[name] = "cluster.deploy"
		}
	}
	return m
}

func validateRestartPolicy(policy string) error {
	switch policy {
	case "always", "on-failure", "never":
		return nil
	default:
		return fmt.Errorf("unknown restart policy %q (must be always, on-failure, or never)", policy)
	}
}

func validateVolumes(vols []string) error {
	for _, v := range vols {
		if strings.Contains(v, "/") {
			// Bind mount: must have host:container
			if strings.Count(v, ":") < 1 {
				return fmt.Errorf("bind mount %q must be in host:container format", v)
			}
		}
		// Named volume: name:/path — at least one colon
		if !strings.Contains(v, ":") {
			return fmt.Errorf("volume %q must be in name:/path or /host:/container format", v)
		}
	}
	return nil
}

func validateAddon(name string, a *AddonConfig) error {
	switch a.Type {
	case "helm":
		if strings.TrimSpace(a.Chart) == "" {
			return fmt.Errorf("chart must not be empty")
		}
		if strings.TrimSpace(a.Namespace) == "" {
			return fmt.Errorf("namespace must not be empty")
		}
	case "manifest":
		if strings.TrimSpace(a.Path) == "" {
			return fmt.Errorf("path must not be empty")
		}
	case "kustomize":
		if strings.TrimSpace(a.Path) == "" {
			return fmt.Errorf("path must not be empty")
		}
	case "":
		return fmt.Errorf("type must be set (helm, manifest, or kustomize)")
	default:
		return fmt.Errorf("unknown addon type %q (must be helm, manifest, or kustomize)", a.Type)
	}
	return nil
}

func validateRetention(s string) error {
	if s == "" {
		return fmt.Errorf("must not be empty")
	}
	// Accept Go duration strings like "1h", "30m", "5m30s"
	_, err := time.ParseDuration(s)
	if err != nil {
		return fmt.Errorf("%q is not a valid duration (e.g. \"1h\", \"30m\"): %v", s, err)
	}
	return nil
}

func detectPortConflicts(cfg *Config) []string {
	seen := make(map[uint16][]string)
	recordOpt := func(port *Port, name string) {
		if port == nil || port.IsAuto() || port.IsZero() {
			return
		}
		seen[port.Value] = append(seen[port.Value], name)
	}
	record := func(port Port, name string) {
		if port.IsAuto() || port.IsZero() {
			return
		}
		seen[port.Value] = append(seen[port.Value], name)
	}

	for name, svc := range cfg.Services {
		recordOpt(svc.Port, fmt.Sprintf("services.%s", name))
	}
	for name, d := range cfg.Docker {
		recordOpt(d.Port, fmt.Sprintf("docker.%s", name))
		for portName, p := range d.Ports {
			record(p, fmt.Sprintf("docker.%s.ports.%s", name, portName))
		}
	}
	if cfg.Dashboard != nil {
		record(cfg.Dashboard.Port, "dashboard")
		if cfg.Dashboard.OTel != nil {
			record(cfg.Dashboard.OTel.GRPCPort, "dashboard.otel.grpc_port")
			record(cfg.Dashboard.OTel.HTTPPort, "dashboard.otel.http_port")
		}
	}

	var errs []string
	for port, owners := range seen {
		if len(owners) > 1 {
			errs = append(errs, fmt.Sprintf("port %d is used by multiple resources: %s", port, strings.Join(owners, ", ")))
		}
	}
	return errs
}

// detectResourceCycles runs DFS across all resources (service, docker, cluster) to find cycles.
func detectResourceCycles(cfg *Config) error {
	type edge struct{ from, to string }
	adj := make(map[string][]string)

	addEdges := func(from string, deps []string) {
		adj[from] = append(adj[from], deps...)
	}

	for name, svc := range cfg.Services {
		addEdges(name, svc.DependsOn)
	}
	for name, d := range cfg.Docker {
		addEdges(name, d.DependsOn)
	}
	if cfg.Cluster != nil {
		for name, img := range cfg.Cluster.Images {
			addEdges(name, img.DependsOn)
		}
		for name, dep := range cfg.Cluster.Deploy {
			addEdges(name, dep.DependsOn)
		}
	}

	visited := make(map[string]bool)
	inStack := make(map[string]bool)

	var dfs func(node string) error
	dfs = func(node string) error {
		visited[node] = true
		inStack[node] = true
		for _, dep := range adj[node] {
			if !visited[dep] {
				if err := dfs(dep); err != nil {
					return err
				}
			} else if inStack[dep] {
				return fmt.Errorf("cycle detected involving %q", dep)
			}
		}
		inStack[node] = false
		return nil
	}

	for node := range adj {
		if !visited[node] {
			if err := dfs(node); err != nil {
				return err
			}
		}
	}
	return nil
}

func detectAddonCycles(addons map[string]AddonConfig) error {
	adj := make(map[string][]string)
	for name, addon := range addons {
		adj[name] = addon.DependsOn
	}

	visited := make(map[string]bool)
	inStack := make(map[string]bool)

	var dfs func(node string) error
	dfs = func(node string) error {
		visited[node] = true
		inStack[node] = true
		for _, dep := range adj[node] {
			if !visited[dep] {
				if err := dfs(dep); err != nil {
					return err
				}
			} else if inStack[dep] {
				return fmt.Errorf("cycle involving %q", dep)
			}
		}
		inStack[node] = false
		return nil
	}

	for node := range adj {
		if !visited[node] {
			if err := dfs(node); err != nil {
				return err
			}
		}
	}
	return nil
}
