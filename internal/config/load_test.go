package config

import (
	"os"
	"path/filepath"
	"testing"
)

// rustEraTOML exercises every config shape the Rust implementation accepted:
// integer ports, "auto" ports, inline ready checks, restart blocks, OIDC
// users/clients, cluster images/deploy/addons, string-or-list commands.
const rustEraTOML = `
[project]
name = "compat-test"

[env]
SHARED = "yes"

[services.api]
command = "go run ./cmd/api"
port = 3000
depends_on = ["postgres"]

[services.api.restart]
policy = "always"

[services.web]
command = "pnpm dev"
port = "auto"

[docker.postgres]
image = "postgres:16"
port = 5432
env = { POSTGRES_PASSWORD = "devrig" }
ready_check = { type = "pg_isready" }
init = ["CREATE DATABASE app;"]

[docker.redis]
image = "redis:7"
port = "auto"
command = "redis-server --appendonly yes"

[dashboard]
port = 4000

[dashboard.otel]
grpc_port = 4317
http_port = 4318

[oidc]
port = "auto"

[[oidc.users]]
email = "admin@example.com"
password = "password123"

[oidc.clients.webapp]
public = true
redirect_uris = ["http://localhost:3000/callback"]

[cluster]
name = "compat"

[cluster.image.worker]
context = "./worker"

[cluster.deploy.api-k8s]
context = "."
manifests = "k8s/"

[cluster.addons.traefik]
type = "helm"
chart = "traefik/traefik"

[cluster.logs]
namespaces = ["default", "apps"]
`

func writeConfig(t *testing.T, name, content string) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestLoadRustEraTOML(t *testing.T) {
	cfg, _, err := Load(writeConfig(t, "devrig.toml", rustEraTOML))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	if cfg.Project.Name != "compat-test" {
		t.Errorf("project name = %q", cfg.Project.Name)
	}

	// Integer port.
	api := cfg.Services["api"]
	if api.Port == nil || api.Port.IsAuto() || api.Port.AsFixed() != 3000 {
		t.Errorf("services.api.port = %+v, want fixed 3000", api.Port)
	}
	// "auto" port.
	web := cfg.Services["web"]
	if web.Port == nil || !web.Port.IsAuto() {
		t.Errorf("services.web.port = %+v, want auto", web.Port)
	}

	// Restart block with defaults filled in for unset fields.
	if api.Restart == nil {
		t.Fatal("services.api.restart is nil")
	}
	if api.Restart.Policy != "always" {
		t.Errorf("restart.policy = %q, want always", api.Restart.Policy)
	}
	if api.Restart.MaxRestarts != 10 {
		t.Errorf("restart.max_restarts default = %d, want 10", api.Restart.MaxRestarts)
	}
	if api.Restart.StartupGraceMs != 2000 {
		t.Errorf("restart.startup_grace_ms default = %d, want 2000", api.Restart.StartupGraceMs)
	}

	// Docker: integer port, ready check, init, string command.
	pg := cfg.Docker["postgres"]
	if pg.Port == nil || pg.Port.AsFixed() != 5432 {
		t.Errorf("docker.postgres.port = %+v, want 5432", pg.Port)
	}
	if pg.ReadyCheck == nil || pg.ReadyCheck.Type != "pg_isready" {
		t.Errorf("docker.postgres.ready_check = %+v", pg.ReadyCheck)
	}
	if len(pg.Init) != 1 {
		t.Errorf("docker.postgres.init = %v", pg.Init)
	}
	redis := cfg.Docker["redis"]
	if redis.Command == nil || len(*redis.Command) != 1 || (*redis.Command)[0] != "redis-server --appendonly yes" {
		t.Errorf("docker.redis.command = %+v, want single-string list", redis.Command)
	}

	// Dashboard + OTel defaults.
	if cfg.Dashboard == nil || cfg.Dashboard.Port.AsFixed() != 4000 {
		t.Fatalf("dashboard = %+v", cfg.Dashboard)
	}
	ot := cfg.Dashboard.OTel
	if ot == nil {
		t.Fatal("dashboard.otel is nil")
	}
	if ot.GRPCPort.AsFixed() != 4317 || ot.HTTPPort.AsFixed() != 4318 {
		t.Errorf("otel ports = %d/%d", ot.GRPCPort.AsFixed(), ot.HTTPPort.AsFixed())
	}
	if ot.TraceBuffer != 10000 || ot.LogBuffer != 100000 || ot.MetricBuffer != 50000 {
		t.Errorf("otel buffer defaults = %d/%d/%d", ot.TraceBuffer, ot.LogBuffer, ot.MetricBuffer)
	}
	if ot.Retention != "1h" {
		t.Errorf("otel retention default = %q", ot.Retention)
	}

	// OIDC: auto port, realm default, users, clients.
	if cfg.OIDC == nil {
		t.Fatal("oidc is nil")
	}
	if !cfg.OIDC.Port.IsAuto() {
		t.Errorf("oidc.port = %+v, want auto", cfg.OIDC.Port)
	}
	if cfg.OIDC.Realm != "devrig" {
		t.Errorf("oidc.realm default = %q, want devrig", cfg.OIDC.Realm)
	}
	if len(cfg.OIDC.Users) != 1 || cfg.OIDC.Users[0].Email != "admin@example.com" {
		t.Errorf("oidc.users = %+v", cfg.OIDC.Users)
	}
	wc, ok := cfg.OIDC.Clients["webapp"]
	if !ok || !wc.Public || len(wc.RedirectURIs) != 1 {
		t.Errorf("oidc.clients.webapp = %+v", wc)
	}

	// Cluster: defaults + addon defaults + namespace filter list.
	if cfg.Cluster == nil {
		t.Fatal("cluster is nil")
	}
	if cfg.Cluster.Agents != 1 {
		t.Errorf("cluster.agents default = %d, want 1", cfg.Cluster.Agents)
	}
	if !cfg.Cluster.Registry {
		t.Error("cluster.registry default = false, want true")
	}
	addon := cfg.Cluster.Addons["traefik"]
	if !addon.Wait || addon.Timeout != "5m" {
		t.Errorf("addon defaults wait=%v timeout=%q", addon.Wait, addon.Timeout)
	}
	img := cfg.Cluster.Images["worker"]
	if img.Dockerfile != "Dockerfile" {
		t.Errorf("image dockerfile default = %q", img.Dockerfile)
	}
	logs := cfg.Cluster.Logs
	if logs == nil || logs.Namespaces.All || len(logs.Namespaces.List) != 2 {
		t.Errorf("cluster.logs.namespaces = %+v", logs)
	}
	if logs != nil && (!logs.Enabled || !logs.Collector) {
		t.Errorf("cluster.logs defaults enabled=%v collector=%v", logs.Enabled, logs.Collector)
	}
}

func TestLoadNamespaceFilterAll(t *testing.T) {
	cfg, _, err := Load(writeConfig(t, "devrig.toml", `
[project]
name = "ns-all"

[cluster]
[cluster.logs]
namespaces = "all"
`))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Cluster.Logs == nil || !cfg.Cluster.Logs.Namespaces.All {
		t.Errorf("namespaces = %+v, want All", cfg.Cluster.Logs)
	}
}

func TestLoadPortOutOfRange(t *testing.T) {
	_, _, err := Load(writeConfig(t, "devrig.toml", `
[project]
name = "bad"

[services.api]
command = "x"
port = 99999
`))
	if err == nil {
		t.Fatal("expected error for out-of-range port")
	}
}

func TestLoadYAMLParity(t *testing.T) {
	cfg, _, err := Load(writeConfig(t, "devrig.yaml", `
project:
  name: yaml-test
services:
  api:
    command: go run .
    port: 3000
  web:
    command: pnpm dev
    port: auto
dashboard:
  port: 4000
`))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Services["api"].Port.AsFixed() != 3000 {
		t.Errorf("yaml api port = %+v", cfg.Services["api"].Port)
	}
	if !cfg.Services["web"].Port.IsAuto() {
		t.Errorf("yaml web port = %+v", cfg.Services["web"].Port)
	}
}
