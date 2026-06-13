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
		ClusterImageTags: map[string]string{"worker": "localhost:5000/worker:123"},
	}
}

func TestInterpolateString(t *testing.T) {
	cases := map[string]string{
		"{{ project.name }}":                    "demo",
		"{{ services.api.port }}":               "3000",
		"{{ docker.postgres.port }}":            "5432",
		"{{ docker.minio.ports.console }}":      "9001",
		"{{ docker.minio.port_console }}":       "9001",
		"{{ dashboard.port }}":                  "4000",
		"{{ oidc.issuer }}":                     "http://localhost:9000",
		"{{ cluster.image.worker.tag }}":        "localhost:5000/worker:123",
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
