package tools

import "github.com/steveyackey/devrig/internal/config"

// ResolverFromConfig builds a Resolver from the devrig [tools] config block.
// A nil block yields defaults (prefer vendored, no overrides). allowFetch
// controls whether missing managed tools are downloaded on demand.
func ResolverFromConfig(c *config.ToolsConfig, allowFetch bool) *Resolver {
	o := Options{AllowFetch: allowFetch, Overrides: map[Tool]string{}}
	if c != nil {
		o.Prefer = c.Prefer
		if c.Kubectl != "" {
			o.Overrides[Kubectl] = c.Kubectl
		}
		if c.Helm != "" {
			o.Overrides[Helm] = c.Helm
		}
		if c.K3d != "" {
			o.Overrides[K3d] = c.K3d
		}
	}
	return NewResolver(o)
}
