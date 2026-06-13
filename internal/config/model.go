package config

import (
	"bytes"
	"fmt"
	"strconv"

	"github.com/BurntSushi/toml"
	"gopkg.in/yaml.v3"
)

// decodeTOMLTable re-decodes a raw TOML table (BurntSushi passes Unmarshaler
// implementations the primitive value: map[string]any, string, int64, …) into
// dst by round-tripping through the TOML encoder, so nested custom types
// (Port, StringOrList, …) decode through their own Unmarshaler.
func decodeTOMLTable(v any, dst any) error {
	m, ok := v.(map[string]any)
	if !ok {
		return fmt.Errorf("expected a table, got %T", v)
	}
	var buf bytes.Buffer
	if err := toml.NewEncoder(&buf).Encode(m); err != nil {
		return fmt.Errorf("re-encode table: %w", err)
	}
	_, err := toml.Decode(buf.String(), dst)
	return err
}

// tomlStringList coerces a raw TOML value into []string when it is either a
// single string or an array of strings.
func tomlStringList(v any) ([]string, bool) {
	switch t := v.(type) {
	case string:
		return []string{t}, true
	case []any:
		out := make([]string, 0, len(t))
		for _, e := range t {
			s, ok := e.(string)
			if !ok {
				return nil, false
			}
			out = append(out, s)
		}
		return out, true
	case []string:
		return t, true
	}
	return nil, false
}

// Config is the top-level devrig.toml / devrig.yaml structure.
type Config struct {
	Project   ProjectConfig            `toml:"project"   yaml:"project"`
	Services  map[string]ServiceConfig `toml:"services"  yaml:"services"`
	Docker    map[string]DockerConfig  `toml:"docker"    yaml:"docker"`
	Compose   *ComposeConfig           `toml:"compose"   yaml:"compose"`
	Cluster   *ClusterConfig           `toml:"cluster"   yaml:"cluster"`
	Dashboard *DashboardConfig         `toml:"dashboard" yaml:"dashboard"`
	OIDC      *OIDCConfig              `toml:"oidc"      yaml:"oidc"`
	Env       map[string]string        `toml:"env"       yaml:"env"`
	Network   *NetworkConfig           `toml:"network"   yaml:"network"`
	Links     map[string]string        `toml:"links"     yaml:"links"`
	Tools     *ToolsConfig             `toml:"tools"     yaml:"tools"`
}

// ToolsConfig controls how devrig resolves the external CLIs its cluster
// features depend on (k3d, kubectl, helm). See docs/prd/managed-tool-deps.md.
type ToolsConfig struct {
	// Prefer is "vendored" (default — devrig-managed, pinned copies) or
	// "system" (whatever is on the user's PATH).
	Prefer string `toml:"prefer" yaml:"prefer"`
	// Per-tool explicit executable path overrides.
	Kubectl string `toml:"kubectl" yaml:"kubectl"`
	Helm    string `toml:"helm"    yaml:"helm"`
	K3d     string `toml:"k3d"     yaml:"k3d"`
}

type ProjectConfig struct {
	Name    string  `toml:"name"     yaml:"name"`
	EnvFile *string `toml:"env_file" yaml:"env_file"`
}

type ServiceConfig struct {
	Path      *string           `toml:"path"       yaml:"path"`
	Command   string            `toml:"command"    yaml:"command"`
	Port      *Port             `toml:"port"       yaml:"port"`
	Protocol  *string           `toml:"protocol"   yaml:"protocol"`
	Env       map[string]string `toml:"env"        yaml:"env"`
	EnvFile   *string           `toml:"env_file"   yaml:"env_file"`
	DependsOn []string          `toml:"depends_on" yaml:"depends_on"`
	Restart   *RestartConfig    `toml:"restart"    yaml:"restart"`
}

type RestartConfig struct {
	Policy             string `toml:"policy"               yaml:"policy"`
	MaxRestarts        uint32 `toml:"max_restarts"         yaml:"max_restarts"`
	StartupMaxRestarts uint32 `toml:"startup_max_restarts" yaml:"startup_max_restarts"`
	StartupGraceMs     uint64 `toml:"startup_grace_ms"     yaml:"startup_grace_ms"`
	InitialDelayMs     uint64 `toml:"initial_delay_ms"     yaml:"initial_delay_ms"`
	MaxDelayMs         uint64 `toml:"max_delay_ms"         yaml:"max_delay_ms"`
}

func defaultRestartConfig() RestartConfig {
	return RestartConfig{
		Policy:             "on-failure",
		MaxRestarts:        10,
		StartupMaxRestarts: 3,
		StartupGraceMs:     2000,
		InitialDelayMs:     500,
		MaxDelayMs:         30000,
	}
}

func (r *RestartConfig) UnmarshalTOML(v any) error {
	*r = defaultRestartConfig()
	type plain RestartConfig
	return decodeTOMLTable(v, (*plain)(r))
}

func (r *RestartConfig) UnmarshalYAML(value *yaml.Node) error {
	*r = defaultRestartConfig()
	type plain RestartConfig
	return value.Decode((*plain)(r))
}

type DockerConfig struct {
	Image         string            `toml:"image"          yaml:"image"`
	Port          *Port             `toml:"port"           yaml:"port"`
	ContainerPort *uint16           `toml:"container_port" yaml:"container_port"`
	Protocol      *string           `toml:"protocol"       yaml:"protocol"`
	Ports         map[string]Port   `toml:"ports"          yaml:"ports"`
	Env           map[string]string `toml:"env"            yaml:"env"`
	Volumes       []string          `toml:"volumes"        yaml:"volumes"`
	Command       *StringOrList     `toml:"command"        yaml:"command"`
	Entrypoint    *StringOrList     `toml:"entrypoint"     yaml:"entrypoint"`
	ReadyCheck    *ReadyCheck       `toml:"ready_check"    yaml:"ready_check"`
	Init          []string          `toml:"init"           yaml:"init"`
	DependsOn     []string          `toml:"depends_on"     yaml:"depends_on"`
	RegistryAuth  *RegistryAuth     `toml:"registry_auth"  yaml:"registry_auth"`
}

// StringOrList is a string or []string in TOML/YAML.
type StringOrList []string

func (s *StringOrList) UnmarshalTOML(v any) error {
	list, ok := tomlStringList(v)
	if !ok {
		return fmt.Errorf("expected string or list of strings, got %T", v)
	}
	*s = list
	return nil
}

func (s *StringOrList) UnmarshalYAML(value *yaml.Node) error {
	var str string
	if err := value.Decode(&str); err == nil {
		*s = StringOrList{str}
		return nil
	}
	var list []string
	if err := value.Decode(&list); err != nil {
		return fmt.Errorf("expected string or list of strings: %w", err)
	}
	*s = list
	return nil
}

type RegistryAuth struct {
	Username string `toml:"username" yaml:"username"`
	Password string `toml:"password" yaml:"password"`
}

// ReadyCheck is a tagged-union health check. The Type field selects the variant.
type ReadyCheck struct {
	Type    string  `toml:"type"    yaml:"type"`
	Command string  `toml:"command" yaml:"command"` // cmd
	Expect  *string `toml:"expect"  yaml:"expect"`  // cmd
	URL     string  `toml:"url"     yaml:"url"`     // http
	Match   string  `toml:"match"   yaml:"match"`   // log
	Timeout *uint64 `toml:"timeout" yaml:"timeout"` // all
}

// TimeoutSecs returns the configured timeout or the variant default.
func (r *ReadyCheck) TimeoutSecs() uint64 {
	if r.Timeout != nil {
		return *r.Timeout
	}
	if r.Type == "log" {
		return 60
	}
	return 30
}

type ComposeConfig struct {
	File        string                 `toml:"file"         yaml:"file"`
	Services    []string               `toml:"services"     yaml:"services"`
	EnvFile     *string                `toml:"env_file"     yaml:"env_file"`
	ReadyChecks map[string]*ReadyCheck `toml:"ready_checks" yaml:"ready_checks"`
}

type NetworkConfig struct {
	Name *string `toml:"name" yaml:"name"`
}

// Port is either a fixed port number (Value > 0) or "auto" (Auto == true).
// The zero value is invalid; use FixedPort or AutoPort to construct.
type Port struct {
	Value uint16
	Auto  bool
}

func FixedPort(p uint16) Port { return Port{Value: p} }
func AutoPort() Port          { return Port{Auto: true} }

func (p Port) IsAuto() bool  { return p.Auto }
func (p Port) AsFixed() uint16 { return p.Value }
func (p Port) IsZero() bool  { return !p.Auto && p.Value == 0 }

func (p *Port) UnmarshalTOML(v any) error {
	switch t := v.(type) {
	case int64:
		if t < 1 || t > 65535 {
			return fmt.Errorf("port %d out of range (1-65535)", t)
		}
		p.Value = uint16(t)
		p.Auto = false
		return nil
	case string:
		if t == "auto" {
			p.Auto = true
			p.Value = 0
			return nil
		}
		n, err := strconv.ParseInt(t, 10, 64)
		if err != nil {
			return fmt.Errorf("expected port number or \"auto\", got %q", t)
		}
		if n < 1 || n > 65535 {
			return fmt.Errorf("port %d out of range (1-65535)", n)
		}
		p.Value = uint16(n)
		p.Auto = false
		return nil
	default:
		return fmt.Errorf("expected port number or \"auto\", got %T", v)
	}
}

func (p *Port) UnmarshalYAML(value *yaml.Node) error {
	var n int
	if err := value.Decode(&n); err == nil {
		if n < 1 || n > 65535 {
			return fmt.Errorf("port %d out of range (1-65535)", n)
		}
		p.Value = uint16(n)
		p.Auto = false
		return nil
	}
	var s string
	if err := value.Decode(&s); err != nil {
		return fmt.Errorf("expected port number or \"auto\"")
	}
	if s == "auto" {
		p.Auto = true
		p.Value = 0
		return nil
	}
	n2, err := strconv.Atoi(s)
	if err != nil {
		return fmt.Errorf("expected port number or \"auto\", got %q", s)
	}
	if n2 < 1 || n2 > 65535 {
		return fmt.Errorf("port %d out of range (1-65535)", n2)
	}
	p.Value = uint16(n2)
	p.Auto = false
	return nil
}

type DashboardConfig struct {
	Port    Port        `toml:"port"    yaml:"port"`
	Enabled *bool       `toml:"enabled" yaml:"enabled"`
	OTel    *OTelConfig `toml:"otel"    yaml:"otel"`
}

func (d *DashboardConfig) UnmarshalTOML(v any) error {
	d.Port = FixedPort(4000)
	type plain DashboardConfig
	return decodeTOMLTable(v, (*plain)(d))
}

func (d *DashboardConfig) UnmarshalYAML(value *yaml.Node) error {
	d.Port = FixedPort(4000)
	type plain DashboardConfig
	return value.Decode((*plain)(d))
}

type OTelConfig struct {
	GRPCPort     Port   `toml:"grpc_port"     yaml:"grpc_port"`
	HTTPPort     Port   `toml:"http_port"     yaml:"http_port"`
	TraceBuffer  int    `toml:"trace_buffer"  yaml:"trace_buffer"`
	MetricBuffer int    `toml:"metric_buffer" yaml:"metric_buffer"`
	LogBuffer    int    `toml:"log_buffer"    yaml:"log_buffer"`
	Retention    string `toml:"retention"     yaml:"retention"`
}

// DefaultOTelConfig returns the OTel settings used when [dashboard.otel] is
// absent: standard OTLP ports, 1h retention, default buffer sizes.
func DefaultOTelConfig() *OTelConfig {
	return &OTelConfig{
		GRPCPort:     FixedPort(4317),
		HTTPPort:     FixedPort(4318),
		TraceBuffer:  10000,
		MetricBuffer: 50000,
		LogBuffer:    100000,
		Retention:    "1h",
	}
}

func (o *OTelConfig) UnmarshalTOML(v any) error {
	*o = *DefaultOTelConfig()
	type plain OTelConfig
	return decodeTOMLTable(v, (*plain)(o))
}

func (o *OTelConfig) UnmarshalYAML(value *yaml.Node) error {
	*o = *DefaultOTelConfig()
	type plain OTelConfig
	return value.Decode((*plain)(o))
}

type OIDCConfig struct {
	Port     Port                        `toml:"port"     yaml:"port"`
	Issuer   *string                     `toml:"issuer"   yaml:"issuer"`
	Realm    string                      `toml:"realm"    yaml:"realm"`
	Audience *string                     `toml:"audience" yaml:"audience"`
	Users    []OIDCUserConfig            `toml:"users"    yaml:"users"`
	Clients  map[string]OIDCClientConfig `toml:"clients"  yaml:"clients"`
}

func (o *OIDCConfig) UnmarshalTOML(v any) error {
	o.Port = AutoPort()
	o.Realm = "devrig"
	type plain OIDCConfig
	return decodeTOMLTable(v, (*plain)(o))
}

func (o *OIDCConfig) UnmarshalYAML(value *yaml.Node) error {
	o.Port = AutoPort()
	o.Realm = "devrig"
	type plain OIDCConfig
	return value.Decode((*plain)(o))
}

type OIDCUserConfig struct {
	Email    string  `toml:"email"    yaml:"email"`
	Password string  `toml:"password" yaml:"password"`
	Name     *string `toml:"name"     yaml:"name"`
	Role     *string `toml:"role"     yaml:"role"`
}

type OIDCClientConfig struct {
	Public       bool     `toml:"public"        yaml:"public"`
	RedirectURIs []string `toml:"redirect_uris" yaml:"redirect_uris"`
	ClientSecret *string  `toml:"client_secret" yaml:"client_secret"`
	ClientName   *string  `toml:"client_name"   yaml:"client_name"`
	GrantTypes   []string `toml:"grant_types"   yaml:"grant_types"`
	Scopes       []string `toml:"scopes"        yaml:"scopes"`
}

type ClusterConfig struct {
	Name       *string                        `toml:"name"       yaml:"name"`
	Agents     uint32                         `toml:"agents"     yaml:"agents"`
	Ports      []string                       `toml:"ports"      yaml:"ports"`
	Volumes    []string                       `toml:"volumes"    yaml:"volumes"`
	Registry   bool                           `toml:"registry"   yaml:"registry"`
	Images     map[string]ClusterImageConfig  `toml:"images"     yaml:"images"`
	Deploy     map[string]ClusterDeployConfig `toml:"deploy"     yaml:"deploy"`
	Addons     map[string]AddonConfig         `toml:"addons"     yaml:"addons"`
	Logs       *ClusterLogsConfig             `toml:"logs"       yaml:"logs"`
	Registries []ClusterRegistryAuth          `toml:"registries" yaml:"registries"`
	K3SArgs    []string                       `toml:"k3s_args"   yaml:"k3s_args"`
}

func (c *ClusterConfig) UnmarshalTOML(v any) error {
	c.Agents = 1
	c.Registry = true
	type plain ClusterConfig
	return decodeTOMLTable(v, (*plain)(c))
}

func (c *ClusterConfig) UnmarshalYAML(value *yaml.Node) error {
	c.Agents = 1
	c.Registry = true
	type plain ClusterConfig
	return value.Decode((*plain)(c))
}

type ClusterRegistryAuth struct {
	URL      string `toml:"url"      yaml:"url"`
	Username string `toml:"username" yaml:"username"`
	Password string `toml:"password" yaml:"password"`
}

type ClusterLogsConfig struct {
	Enabled           bool            `toml:"enabled"            yaml:"enabled"`
	Collector         bool            `toml:"collector"          yaml:"collector"`
	Namespaces        NamespaceFilter `toml:"namespaces"         yaml:"namespaces"`
	ExcludeNamespaces []string        `toml:"exclude_namespaces" yaml:"exclude_namespaces"`
	ExcludePods       []string        `toml:"exclude_pods"       yaml:"exclude_pods"`
}

func (l *ClusterLogsConfig) UnmarshalTOML(v any) error {
	l.Enabled = true
	l.Collector = true
	l.Namespaces = NamespaceFilter{List: []string{"default"}}
	type plain ClusterLogsConfig
	return decodeTOMLTable(v, (*plain)(l))
}

func (l *ClusterLogsConfig) UnmarshalYAML(value *yaml.Node) error {
	l.Enabled = true
	l.Collector = true
	l.Namespaces = NamespaceFilter{List: []string{"default"}}
	type plain ClusterLogsConfig
	return value.Decode((*plain)(l))
}

// NamespaceFilter is either the string "all" or a list of namespace names.
type NamespaceFilter struct {
	All  bool
	List []string
}

func (n *NamespaceFilter) UnmarshalTOML(v any) error {
	if s, ok := v.(string); ok {
		if s != "all" {
			return fmt.Errorf("expected \"all\" or a list of namespaces, got %q", s)
		}
		n.All = true
		return nil
	}
	list, ok := tomlStringList(v)
	if !ok {
		return fmt.Errorf("expected \"all\" or a list of namespaces, got %T", v)
	}
	n.List = list
	return nil
}

func (n *NamespaceFilter) UnmarshalYAML(value *yaml.Node) error {
	var s string
	if err := value.Decode(&s); err == nil {
		if s != "all" {
			return fmt.Errorf("expected \"all\" or a list of namespaces, got %q", s)
		}
		n.All = true
		return nil
	}
	var list []string
	if err := value.Decode(&list); err != nil {
		return fmt.Errorf("expected \"all\" or a list of namespaces: %w", err)
	}
	n.List = list
	return nil
}

// AddonConfig is a tagged-union addon (helm/manifest/kustomize).
// The Type field determines which other fields are relevant.
type AddonConfig struct {
	Type        string            `toml:"type"         yaml:"type"`
	Chart       string            `toml:"chart"        yaml:"chart"`        // helm
	Repo        *string           `toml:"repo"         yaml:"repo"`         // helm
	Namespace   string            `toml:"namespace"    yaml:"namespace"`    // helm/manifest/kustomize
	Version     *string           `toml:"version"      yaml:"version"`      // helm
	Values      map[string]any    `toml:"values"       yaml:"values"`       // helm
	ValuesFiles []string          `toml:"values_files" yaml:"values_files"` // helm
	Wait        bool              `toml:"wait"         yaml:"wait"`         // helm
	Timeout     string            `toml:"timeout"      yaml:"timeout"`      // helm
	SkipCRDs    bool              `toml:"skip_crds"    yaml:"skip_crds"`    // helm
	Path        string            `toml:"path"         yaml:"path"`         // manifest/kustomize
	PortForward map[string]string `toml:"port_forward" yaml:"port_forward"` // all
	DependsOn   []string          `toml:"depends_on"   yaml:"depends_on"`   // all
}

func (a *AddonConfig) UnmarshalTOML(v any) error {
	a.Wait = true
	a.Timeout = "5m"
	type plain AddonConfig
	return decodeTOMLTable(v, (*plain)(a))
}

func (a *AddonConfig) UnmarshalYAML(value *yaml.Node) error {
	a.Wait = true
	a.Timeout = "5m"
	type plain AddonConfig
	return value.Decode((*plain)(a))
}

// ParsedPortForwards returns (localPort, target) pairs from the port_forward map.
func (a *AddonConfig) ParsedPortForwards() []struct {
	Local  uint16
	Target string
} {
	var out []struct {
		Local  uint16
		Target string
	}
	for k, v := range a.PortForward {
		var port uint16
		if _, err := fmt.Sscanf(k, "%d", &port); err == nil {
			out = append(out, struct {
				Local  uint16
				Target string
			}{port, v})
		}
	}
	return out
}

type ClusterImageConfig struct {
	Context      string            `toml:"context"       yaml:"context"`
	Dockerfile   string            `toml:"dockerfile"    yaml:"dockerfile"`
	Watch        bool              `toml:"watch"         yaml:"watch"`
	DependsOn    []string          `toml:"depends_on"    yaml:"depends_on"`
	BuildSecrets map[string]string `toml:"build_secrets" yaml:"build_secrets"`
	BuildArgs    map[string]string `toml:"build_args"    yaml:"build_args"`
}

func (c *ClusterImageConfig) UnmarshalTOML(v any) error {
	c.Dockerfile = "Dockerfile"
	type plain ClusterImageConfig
	return decodeTOMLTable(v, (*plain)(c))
}

func (c *ClusterImageConfig) UnmarshalYAML(value *yaml.Node) error {
	c.Dockerfile = "Dockerfile"
	type plain ClusterImageConfig
	return value.Decode((*plain)(c))
}

type ClusterDeployConfig struct {
	Context      string            `toml:"context"       yaml:"context"`
	Dockerfile   string            `toml:"dockerfile"    yaml:"dockerfile"`
	Manifests    string            `toml:"manifests"     yaml:"manifests"`
	Watch        bool              `toml:"watch"         yaml:"watch"`
	DependsOn    []string          `toml:"depends_on"    yaml:"depends_on"`
	BuildSecrets map[string]string `toml:"build_secrets" yaml:"build_secrets"`
}

func (c *ClusterDeployConfig) UnmarshalTOML(v any) error {
	c.Dockerfile = "Dockerfile"
	type plain ClusterDeployConfig
	return decodeTOMLTable(v, (*plain)(c))
}

func (c *ClusterDeployConfig) UnmarshalYAML(value *yaml.Node) error {
	c.Dockerfile = "Dockerfile"
	type plain ClusterDeployConfig
	return value.Decode((*plain)(c))
}

// Ensure packages are referenced.
var _ = toml.Unmarshal
var _ = yaml.Unmarshal
