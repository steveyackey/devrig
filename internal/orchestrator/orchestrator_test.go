package orchestrator

import (
	"testing"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/state"
)

// buildTemplateVars must expose cluster.image.<name>.tag as just the tag
// portion of the full image ref (e.g. "1234567890"), matching what service env
// and addon values expect. Regression: the Go rewrite originally never
// populated this map, and an interim fix stored the full ref by mistake.
func TestBuildTemplateVarsClusterImageTags(t *testing.T) {
	regName := "k3d-devrig-x-reg"
	regPort := uint16(5000)
	cs := &state.ClusterState{
		ClusterName:    "devrig-x",
		KubeconfigPath: "/tmp/kc",
		RegistryName:   &regName,
		RegistryPort:   &regPort,
		DeployedServices: map[string]state.ClusterDeployState{
			"agent":      {ImageTag: "localhost:5000/agent:1781529990"},
			"agent-base": {ImageTag: "localhost:5000/agent-base:1781529984"},
			"nopush":     {ImageTag: "devrig-nopush:latest"},
		},
	}
	cfg := &config.Config{Project: config.ProjectConfig{Name: "x"}}

	tv := buildTemplateVars(cfg, nil, nil, nil, cs, 0)

	want := map[string]string{
		"agent":      "1781529990",
		"agent-base": "1781529984",
		"nopush":     "latest",
	}
	for name, w := range want {
		if got := tv.ClusterImageTags[name]; got != w {
			t.Errorf("ClusterImageTags[%q] = %q, want %q", name, got, w)
		}
	}
}
