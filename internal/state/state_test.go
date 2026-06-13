package state

import (
	"testing"
	"time"
)

func TestSaveLoadRoundTrip(t *testing.T) {
	dir := t.TempDir()
	port := uint16(3000)
	phase := "running"
	s := &ProjectState{
		Slug:       "test-abc123",
		ConfigPath: "/tmp/devrig.toml",
		StartedAt:  time.Now().UTC().Truncate(time.Second),
		Services: map[string]ServiceState{
			"api": {PID: 1234, Port: &port, Phase: &phase},
		},
		Docker: map[string]DockerState{
			"postgres": {ContainerID: "abc", ContainerName: "devrig-test-postgres", InitCompleted: true},
		},
		ComposeServices: map[string]ComposeServiceState{},
		PID:             999,
	}
	if err := s.Save(dir); err != nil {
		t.Fatal(err)
	}

	got := Load(dir)
	if got == nil {
		t.Fatal("Load returned nil")
	}
	if got.Slug != s.Slug || got.PID != 999 {
		t.Errorf("round trip mismatch: %+v", got)
	}
	api := got.Services["api"]
	if api.PID != 1234 || api.Port == nil || *api.Port != 3000 || api.Phase == nil || *api.Phase != "running" {
		t.Errorf("service state mismatch: %+v", api)
	}
	if !got.Docker["postgres"].InitCompleted {
		t.Error("docker init_completed lost")
	}
}

func TestLoadMissingReturnsNil(t *testing.T) {
	if got := Load(t.TempDir()); got != nil {
		t.Errorf("expected nil, got %+v", got)
	}
}

func TestUpdateServicePhaseCreatesEntry(t *testing.T) {
	dir := t.TempDir()
	s := &ProjectState{Slug: "x", Services: map[string]ServiceState{}}
	if err := s.Save(dir); err != nil {
		t.Fatal(err)
	}

	UpdateServicePhase(dir, "api", "running")
	UpdateServicePID(dir, "api", 4321)
	UpdateServiceExit(dir, "api", 0)

	got := Load(dir)
	if got == nil {
		t.Fatal("Load returned nil")
	}
	api := got.Services["api"]
	if api.Phase == nil || *api.Phase != "running" {
		t.Errorf("phase = %+v", api.Phase)
	}
	if api.PID != 4321 {
		t.Errorf("pid = %d", api.PID)
	}
	if api.ExitCode == nil || *api.ExitCode != 0 {
		t.Errorf("exit code = %+v", api.ExitCode)
	}
	// Slug must survive the partial updates.
	if got.Slug != "x" {
		t.Errorf("slug lost: %q", got.Slug)
	}
}
