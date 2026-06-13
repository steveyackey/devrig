package graph

import (
	"strings"
	"testing"

	"github.com/steveyackey/devrig/internal/config"
)

func svc(deps ...string) config.ServiceConfig {
	return config.ServiceConfig{Command: "true", DependsOn: deps}
}

func TestStartOrderRespectsDependencies(t *testing.T) {
	cfg := &config.Config{
		Services: map[string]config.ServiceConfig{
			"api": svc("postgres"),
			"web": svc("api"),
		},
		Docker: map[string]config.DockerConfig{
			"postgres": {Image: "postgres:16"},
		},
	}
	r, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	order, err := r.StartOrder()
	if err != nil {
		t.Fatal(err)
	}
	pos := make(map[string]int)
	for i, n := range order {
		pos[n.Name] = i
	}
	if !(pos["postgres"] < pos["api"] && pos["api"] < pos["web"]) {
		t.Errorf("bad order: %v", order)
	}
}

func TestCycleDetection(t *testing.T) {
	cfg := &config.Config{
		Services: map[string]config.ServiceConfig{
			"a": svc("b"),
			"b": svc("a"),
		},
	}
	r, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := r.StartOrder(); err == nil || !strings.Contains(err.Error(), "cycle") {
		t.Errorf("expected cycle error, got %v", err)
	}
}

func TestUnknownDependency(t *testing.T) {
	cfg := &config.Config{
		Services: map[string]config.ServiceConfig{
			"api": svc("ghost"),
		},
	}
	if _, err := New(cfg); err == nil || !strings.Contains(err.Error(), "ghost") {
		t.Errorf("expected unknown-dep error, got %v", err)
	}
}

func TestMixedResourceKinds(t *testing.T) {
	cfg := &config.Config{
		Services: map[string]config.ServiceConfig{"api": svc()},
		Docker:   map[string]config.DockerConfig{"db": {Image: "postgres"}},
		Compose:  &config.ComposeConfig{Services: []string{"queue"}},
		Cluster: &config.ClusterConfig{
			Images: map[string]config.ClusterImageConfig{"img": {}},
			Deploy: map[string]config.ClusterDeployConfig{"dep": {DependsOn: []string{"img"}}},
		},
	}
	r, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	order, err := r.StartOrder()
	if err != nil {
		t.Fatal(err)
	}
	if len(order) != 5 {
		t.Errorf("expected 5 nodes, got %d", len(order))
	}
	kinds := map[string]ResourceKind{}
	for _, n := range order {
		kinds[n.Name] = n.Kind
	}
	if kinds["db"] != KindDocker || kinds["queue"] != KindCompose || kinds["img"] != KindClusterImage {
		t.Errorf("kinds wrong: %v", kinds)
	}
}
