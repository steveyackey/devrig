package config

import (
	"fmt"
	"regexp"
	"strings"
)

var templateRe = regexp.MustCompile(`\{\{\s*([\w.\-]+)\s*\}\}`)

// TemplateVars holds all resolved values available for {{ }} interpolation.
// Values are populated across the orchestrator startup phases.
type TemplateVars struct {
	ProjectName string
	// services.NAME.port
	ServicePorts map[string]uint16
	// docker.NAME.port (primary port)
	DockerPorts map[string]uint16
	// docker.NAME.ports.PORTNAME (named ports)
	DockerNamedPorts map[string]map[string]uint16
	// compose.NAME.port
	ComposePorts map[string]uint16
	// dashboard.*
	DashboardPort *uint16
	OTelGRPCPort  *uint16
	OTelHTTPPort  *uint16
	// oidc.*
	OIDCPort   *uint16
	OIDCIssuer *string
	// cluster.*
	ClusterName         *string
	ClusterRegistry     *string
	ClusterRegistryHost *string
	ClusterKubeconfig   *string
	// cluster.image.NAME.tag
	ClusterImageTags map[string]string
}

// InterpolateString replaces all {{ path.to.var }} expressions in s using vars.
// Returns an error listing all unresolved variables.
func InterpolateString(s string, vars *TemplateVars) (string, error) {
	var missing []string
	result := templateRe.ReplaceAllStringFunc(s, func(match string) string {
		sub := templateRe.FindStringSubmatch(match)
		if len(sub) < 2 {
			return match
		}
		key := sub[1]
		val, ok := resolveVar(key, vars)
		if !ok {
			missing = append(missing, key)
			suggestion := suggestVar(key, vars)
			if suggestion != "" {
				missing[len(missing)-1] = fmt.Sprintf("%s (did you mean {{ %s }}?)", key, suggestion)
			}
			return match
		}
		return val
	})
	if len(missing) > 0 {
		return "", fmt.Errorf("unresolved template variables: %s", strings.Join(missing, ", "))
	}
	return result, nil
}

// InterpolateMap interpolates all values in a string map in-place.
func InterpolateMap(m map[string]string, vars *TemplateVars) error {
	var errs []string
	for k, v := range m {
		resolved, err := InterpolateString(v, vars)
		if err != nil {
			errs = append(errs, fmt.Sprintf("  %s: %v", k, err))
			continue
		}
		m[k] = resolved
	}
	if len(errs) > 0 {
		return fmt.Errorf("%s", strings.Join(errs, "\n"))
	}
	return nil
}

// InterpolateSlice interpolates each string in a slice.
func InterpolateSlice(s []string, vars *TemplateVars) ([]string, error) {
	out := make([]string, len(s))
	var errs []string
	for i, v := range s {
		resolved, err := InterpolateString(v, vars)
		if err != nil {
			errs = append(errs, fmt.Sprintf("  [%d]: %v", i, err))
			out[i] = v
			continue
		}
		out[i] = resolved
	}
	if len(errs) > 0 {
		return out, fmt.Errorf("%s", strings.Join(errs, "\n"))
	}
	return out, nil
}

// interpolateAny recursively interpolates {{ }} templates in a value taken from
// an addon's `values` map. The value may be a string, a nested map, or a slice
// (BurntSushi decodes TOML tables to map[string]any and table arrays to
// []map[string]any). Non-string scalars (bool/int/float) pass through.
func interpolateAny(v any, vars *TemplateVars) (any, error) {
	switch t := v.(type) {
	case string:
		return InterpolateString(t, vars)
	case map[string]any:
		for k, val := range t {
			r, err := interpolateAny(val, vars)
			if err != nil {
				return nil, fmt.Errorf("%s: %w", k, err)
			}
			t[k] = r
		}
		return t, nil
	case []any:
		for i, val := range t {
			r, err := interpolateAny(val, vars)
			if err != nil {
				return nil, fmt.Errorf("[%d]: %w", i, err)
			}
			t[i] = r
		}
		return t, nil
	case []map[string]any:
		for i, val := range t {
			r, err := interpolateAny(val, vars)
			if err != nil {
				return nil, fmt.Errorf("[%d]: %w", i, err)
			}
			t[i] = r.(map[string]any)
		}
		return t, nil
	default:
		return v, nil
	}
}

func resolveVar(key string, vars *TemplateVars) (string, bool) {
	parts := strings.Split(key, ".")
	switch parts[0] {
	case "project":
		if len(parts) == 2 && parts[1] == "name" {
			return vars.ProjectName, true
		}
	case "services":
		if len(parts) == 3 && parts[2] == "port" {
			if p, ok := vars.ServicePorts[parts[1]]; ok {
				return fmt.Sprint(p), true
			}
		}
	case "docker":
		if len(parts) < 3 {
			break
		}
		name := parts[1]
		// docker.NAME.ports.PORTNAME (canonical)
		if len(parts) == 4 && parts[2] == "ports" {
			if named, ok := vars.DockerNamedPorts[name]; ok {
				if p, ok := named[parts[3]]; ok {
					return fmt.Sprint(p), true
				}
			}
			break
		}
		if len(parts) == 3 {
			switch {
			case parts[2] == "port":
				if p, ok := vars.DockerPorts[name]; ok {
					return fmt.Sprint(p), true
				}
			case strings.HasPrefix(parts[2], "port_"):
				// docker.NAME.port_PORTNAME (short alias, Rust parity)
				if named, ok := vars.DockerNamedPorts[name]; ok {
					if p, ok := named[strings.TrimPrefix(parts[2], "port_")]; ok {
						return fmt.Sprint(p), true
					}
				}
			}
		}
	case "compose":
		if len(parts) == 3 && parts[2] == "port" {
			if p, ok := vars.ComposePorts[parts[1]]; ok {
				return fmt.Sprint(p), true
			}
		}
	case "dashboard":
		if len(parts) == 2 {
			switch parts[1] {
			case "port":
				if vars.DashboardPort != nil {
					return fmt.Sprint(*vars.DashboardPort), true
				}
			}
		}
		if len(parts) == 3 && parts[1] == "otel" {
			switch parts[2] {
			case "grpc_port":
				if vars.OTelGRPCPort != nil {
					return fmt.Sprint(*vars.OTelGRPCPort), true
				}
			case "http_port":
				if vars.OTelHTTPPort != nil {
					return fmt.Sprint(*vars.OTelHTTPPort), true
				}
			}
		}
	case "oidc":
		if len(parts) == 2 {
			switch parts[1] {
			case "port":
				if vars.OIDCPort != nil {
					return fmt.Sprint(*vars.OIDCPort), true
				}
			case "issuer":
				if vars.OIDCIssuer != nil {
					return *vars.OIDCIssuer, true
				}
			}
		}
	case "cluster":
		if len(parts) == 2 {
			switch parts[1] {
			case "name":
				if vars.ClusterName != nil {
					return *vars.ClusterName, true
				}
			case "registry":
				if vars.ClusterRegistry != nil {
					return *vars.ClusterRegistry, true
				}
			case "registry_host":
				if vars.ClusterRegistryHost != nil {
					return *vars.ClusterRegistryHost, true
				}
			case "kubeconfig":
				if vars.ClusterKubeconfig != nil {
					return *vars.ClusterKubeconfig, true
				}
			}
		}
		if len(parts) == 4 && parts[1] == "image" && parts[3] == "tag" {
			if tag, ok := vars.ClusterImageTags[parts[2]]; ok {
				return tag, true
			}
		}
	}
	return "", false
}

// suggestVar finds the closest known variable name for a typo hint.
// Uses a simple substring / prefix match rather than full Jaro-Winkler.
func suggestVar(key string, vars *TemplateVars) string {
	known := knownVars(vars)
	best := ""
	bestScore := 0
	for _, candidate := range known {
		score := similarity(key, candidate)
		if score > bestScore {
			bestScore = score
			best = candidate
		}
	}
	if bestScore >= 3 {
		return best
	}
	return ""
}

func similarity(a, b string) int {
	score := 0
	if strings.HasPrefix(b, strings.Split(a, ".")[0]) {
		score += 2
	}
	for _, part := range strings.Split(a, ".") {
		if strings.Contains(b, part) {
			score++
		}
	}
	return score
}

// InterpolateConfig resolves {{ }} expressions in all interpolatable string
// fields of the config. It runs after port allocation so all template vars
// are available. Only services in `only` are interpolated; pass nil to
// interpolate everything.
func InterpolateConfig(cfg *Config, vars *TemplateVars) error {
	var errs []string

	interpStr := func(label, s string) string {
		if !strings.Contains(s, "{{") {
			return s
		}
		r, err := InterpolateString(s, vars)
		if err != nil {
			errs = append(errs, fmt.Sprintf("  %s: %v", label, err))
			return s
		}
		return r
	}

	interpMap := func(label string, m map[string]string) {
		for k, v := range m {
			m[k] = interpStr(fmt.Sprintf("%s.%s", label, k), v)
		}
	}

	for name, svc := range cfg.Services {
		svc.Command = interpStr(fmt.Sprintf("services.%s.command", name), svc.Command)
		if svc.Path != nil {
			s := interpStr(fmt.Sprintf("services.%s.path", name), *svc.Path)
			svc.Path = &s
		}
		interpMap(fmt.Sprintf("services.%s.env", name), svc.Env)
		cfg.Services[name] = svc
	}

	for name, d := range cfg.Docker {
		d.Image = interpStr(fmt.Sprintf("docker.%s.image", name), d.Image)
		interpMap(fmt.Sprintf("docker.%s.env", name), d.Env)
		cfg.Docker[name] = d
	}

	// Project-level [env].
	interpMap("env", cfg.Env)

	// OIDC client redirect URIs commonly reference a service's resolved port,
	// e.g. "http://localhost:{{ services.web.port }}/auth/callback". They are
	// seeded into the provider, so they must be resolved or the OAuth client's
	// redirect_uri won't match ("redirect_uri is not registered").
	if cfg.OIDC != nil {
		for id, c := range cfg.OIDC.Clients {
			for i, ru := range c.RedirectURIs {
				c.RedirectURIs[i] = interpStr(fmt.Sprintf("oidc.clients.%s.redirect_uris[%d]", id, i), ru)
			}
			cfg.OIDC.Clients[id] = c
		}
	}

	if len(errs) > 0 {
		return fmt.Errorf("%s", strings.Join(errs, "\n"))
	}
	return nil
}

// InterpolateAddonValues resolves {{ }} templates in every helm addon's
// `values`. Addons commonly reference cluster vars (e.g. "image.tag" =
// "{{ cluster.image.NAME.tag }}"). Addons install during the cluster phase,
// before the main InterpolateConfig pass runs, so the orchestrator calls this
// separately at that point — once the cluster registry and image tags are
// known. Values is map[string]any, so string leaves are interpolated
// recursively (through nested maps/slices); the map is mutated in place.
func InterpolateAddonValues(addons map[string]AddonConfig, vars *TemplateVars) error {
	var errs []string
	for name, addon := range addons {
		for k, v := range addon.Values {
			r, err := interpolateAny(v, vars)
			if err != nil {
				errs = append(errs, fmt.Sprintf("  cluster.addons.%s.values.%s: %v", name, k, err))
				continue
			}
			addon.Values[k] = r
		}
	}
	if len(errs) > 0 {
		return fmt.Errorf("%s", strings.Join(errs, "\n"))
	}
	return nil
}

func knownVars(vars *TemplateVars) []string {
	var out []string
	out = append(out, "project.name")
	for name := range vars.ServicePorts {
		out = append(out, fmt.Sprintf("services.%s.port", name))
	}
	for name := range vars.DockerPorts {
		out = append(out, fmt.Sprintf("docker.%s.port", name))
	}
	for name, named := range vars.DockerNamedPorts {
		for portName := range named {
			out = append(out, fmt.Sprintf("docker.%s.%s", name, portName))
		}
	}
	for name := range vars.ComposePorts {
		out = append(out, fmt.Sprintf("compose.%s.port", name))
	}
	if vars.DashboardPort != nil {
		out = append(out, "dashboard.port", "dashboard.otel.grpc_port", "dashboard.otel.http_port")
	}
	if vars.OIDCPort != nil {
		out = append(out, "oidc.port", "oidc.issuer")
	}
	if vars.ClusterName != nil {
		out = append(out, "cluster.name", "cluster.registry", "cluster.registry_host", "cluster.kubeconfig")
	}
	for name := range vars.ClusterImageTags {
		out = append(out, fmt.Sprintf("cluster.image.%s.tag", name))
	}
	return out
}
