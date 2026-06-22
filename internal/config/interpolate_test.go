package config

import (
	"strings"
	"testing"
)

func testVars() *TemplateVars {
	dash := uint16(4000)
	httpPort := uint16(4318)
	oidcPort := uint16(9000)
	issuer := "http://localhost:9000"
	return &TemplateVars{
		ProjectName:  "demo",
		ServicePorts: map[string]uint16{"api": 3000},
		DockerPorts:  map[string]uint16{"postgres": 5432},
		DockerNamedPorts: map[string]map[string]uint16{
			"minio": {"console": 9001},
		},
		DashboardPort:    &dash,
		OTelHTTPPort:     &httpPort,
		OIDCPort:         &oidcPort,
		OIDCIssuer:       &issuer,
		ClusterImageTags: map[string]string{"worker": "123"},
	}
}

func TestInterpolateString(t *testing.T) {
	cases := map[string]string{
		"{{ project.name }}":                                     "demo",
		"{{ services.api.port }}":                                "3000",
		"{{ docker.postgres.port }}":                             "5432",
		"{{ docker.minio.ports.console }}":                       "9001",
		"{{ docker.minio.port_console }}":                        "9001",
		"{{ dashboard.port }}":                                   "4000",
		"{{ oidc.issuer }}":                                      "http://localhost:9000",
		"{{ cluster.image.worker.tag }}":                         "123",
		"postgres://u:p@localhost:{{ docker.postgres.port }}/db": "postgres://u:p@localhost:5432/db",
	}
	vars := testVars()
	for in, want := range cases {
		got, err := InterpolateString(in, vars)
		if err != nil {
			t.Errorf("%s: %v", in, err)
			continue
		}
		if got != want {
			t.Errorf("%s = %q, want %q", in, got, want)
		}
	}
}

func TestInterpolateUnknownVarSuggests(t *testing.T) {
	_, err := InterpolateString("{{ services.api.prot }}", testVars())
	if err == nil {
		t.Fatal("expected error for unknown var")
	}
	if !strings.Contains(err.Error(), "services.api.prot") {
		t.Errorf("error doesn't name the bad var: %v", err)
	}
}

func TestInterpolateMapCollectsErrors(t *testing.T) {
	m := map[string]string{
		"GOOD": "{{ project.name }}",
		"BAD":  "{{ nope.nope }}",
	}
	err := InterpolateMap(m, testVars())
	if err == nil {
		t.Fatal("expected error")
	}
	if m["GOOD"] != "demo" {
		t.Errorf("good value not interpolated: %q", m["GOOD"])
	}
}

func TestInterpolateConfigResolvesOIDCRedirectURIsAndEnv(t *testing.T) {
	web := uint16(53255)
	vars := &TemplateVars{ServicePorts: map[string]uint16{"web": web}}
	cfg := &Config{
		Env: map[string]string{"CALLBACK": "http://localhost:{{ services.web.port }}/cb"},
		OIDC: &OIDCConfig{
			Clients: map[string]OIDCClientConfig{
				"web": {RedirectURIs: []string{
					"http://localhost:{{ services.web.port }}/auth/callback",
					"http://localhost:{{ services.web.port }}/",
				}},
			},
		},
	}
	if err := InterpolateConfig(cfg, vars); err != nil {
		t.Fatalf("InterpolateConfig: %v", err)
	}
	got := cfg.OIDC.Clients["web"].RedirectURIs
	want := []string{"http://localhost:53255/auth/callback", "http://localhost:53255/"}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("redirect_uris[%d] = %q, want %q", i, got[i], want[i])
		}
	}
	if cfg.Env["CALLBACK"] != "http://localhost:53255/cb" {
		t.Errorf("env CALLBACK = %q", cfg.Env["CALLBACK"])
	}
}

func TestInterpolateAddonValues(t *testing.T) {
	reg := "k3d-reg:5000"
	vars := &TemplateVars{
		ClusterRegistry:  &reg,
		ClusterImageTags: map[string]string{"theoven": "1782150496"},
	}
	addons := map[string]AddonConfig{
		"app": {
			Values: map[string]any{
				"image.repository": "{{ cluster.registry }}/theoven",
				"image.tag":        "{{ cluster.image.theoven.tag }}",
				"nested":           map[string]any{"url": "http://{{ cluster.registry }}/x"},
				"replicaCount":     int64(2),
				"redis.enabled":    true,
			},
		},
	}
	if err := InterpolateAddonValues(addons, vars); err != nil {
		t.Fatalf("InterpolateAddonValues: %v", err)
	}
	v := addons["app"].Values
	if v["image.repository"] != "k3d-reg:5000/theoven" {
		t.Errorf("image.repository = %v", v["image.repository"])
	}
	if v["image.tag"] != "1782150496" {
		t.Errorf("image.tag = %v", v["image.tag"])
	}
	if nested := v["nested"].(map[string]any); nested["url"] != "http://k3d-reg:5000/x" {
		t.Errorf("nested.url = %v", nested["url"])
	}
	// Non-string scalars pass through unchanged.
	if v["replicaCount"] != int64(2) || v["redis.enabled"] != true {
		t.Errorf("scalars changed: %v %v", v["replicaCount"], v["redis.enabled"])
	}
}

func TestInterpolateAddonValuesReportsUnresolved(t *testing.T) {
	addons := map[string]AddonConfig{
		"app": {Values: map[string]any{"image.tag": "{{ cluster.image.nope.tag }}"}},
	}
	err := InterpolateAddonValues(addons, &TemplateVars{})
	if err == nil || !strings.Contains(err.Error(), "cluster.addons.app.values.image.tag") {
		t.Fatalf("expected unresolved-var error mentioning the addon value, got: %v", err)
	}
}
