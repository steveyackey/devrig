package commands

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"
)

func NewInitCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "init",
		Short: "Generate a starter devrig.toml",
		RunE: func(cmd *cobra.Command, args []string) error {
			cwd, err := os.Getwd()
			if err != nil {
				return err
			}
			dest := filepath.Join(cwd, "devrig.toml")
			if _, err := os.Stat(dest); err == nil {
				return fmt.Errorf("devrig.toml already exists in %s", cwd)
			}

			projectName := filepath.Base(cwd)
			serviceName, serviceCommand := detectProjectType(cwd)

			content := buildTemplate(projectName, serviceName, serviceCommand)
			if err := os.WriteFile(dest, []byte(content), 0o644); err != nil {
				return fmt.Errorf("write devrig.toml: %w", err)
			}
			fmt.Fprintf(cmd.OutOrStdout(), "Created %s\n", dest)
			return nil
		},
	}
}

func detectProjectType(dir string) (serviceName, command string) {
	switch {
	case fileExists(dir, "Cargo.toml"):
		return "api", "cargo watch -x run"
	case fileExists(dir, "package.json"):
		return "app", "pnpm dev"
	case fileExists(dir, "go.mod"):
		return "server", "go run ./..."
	case fileExists(dir, "pyproject.toml"), fileExists(dir, "requirements.txt"):
		return "app", "python -m uvicorn main:app --reload"
	default:
		return "app", "echo 'replace me'"
	}
}

func fileExists(dir, name string) bool {
	_, err := os.Stat(filepath.Join(dir, name))
	return err == nil
}

func buildTemplate(projectName, serviceName, serviceCommand string) string {
	t := "[project]\n"
	t += fmt.Sprintf("name = %q\n", projectName)
	t += "# env_file = \".env\"            # Load shared secrets from a .env file\n"
	t += "\n"
	t += "# -- Global env vars shared by all services (supports {{ }} templates) --\n"
	t += "# [env]\n"
	t += "# RUST_LOG = \"debug\"\n"
	t += "# NODE_ENV = \"development\"\n"
	t += "# SECRET_KEY = \"$MY_SECRET_KEY\" # $VAR expands from .env or host environment\n"
	t += fmt.Sprintf("# DATABASE_URL = \"postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/%s\"\n", projectName)
	t += "\n"
	t += "# -- Dashboard + OpenTelemetry --\n"
	t += "# Built-in dashboard and OTel collector. Services automatically receive\n"
	t += "# OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_SERVICE_NAME. Ports auto-resolve\n"
	t += "# if already in use, so multiple devrig instances can coexist.\n"
	t += "[dashboard]\n"
	t += "# port = 4000                    # default; auto-resolves if in use\n"
	t += "# OTel defaults: grpc_port=4317, http_port=4318, retention=\"1h\" -- customize with [dashboard.otel]\n"
	t += "\n"
	t += "# -- OIDC provider (built-in, replaces Keycloak/dex for local dev) --\n"
	t += "# Spins up an in-process OAuth2 / OIDC provider with pre-seeded users + clients.\n"
	t += "# Services receive 'oidc.issuer' and 'oidc.port' template vars.\n"
	t += "#\n"
	t += "# [oidc]\n"
	t += "# port = \"auto\"\n"
	t += fmt.Sprintf("# realm = %q\n", projectName)
	t += fmt.Sprintf("# audience = \"%s-api\"   # written as `aud` on access tokens\n", projectName)
	t += "#\n"
	t += "# [[oidc.users]]\n"
	t += "# email = \"admin@example.com\"\n"
	t += "# password = \"admin\"\n"
	t += "# name = \"Admin\"\n"
	t += "# role = \"admin\"\n"
	t += "#\n"
	t += "# [oidc.clients.web]\n"
	t += "# public = true                                                      # PKCE, no client_secret\n"
	t += fmt.Sprintf("# redirect_uris = [\"http://localhost:{{ services.%s.port }}/auth/callback\"]\n", serviceName)
	t += "\n"
	t += "# -- Links --\n"
	t += "# Named URLs for services devrig doesn't manage (shown in dashboard).\n"
	t += "# [links]\n"
	t += "# headlamp = \"http://localhost:8080\"\n"
	t += "# grafana = \"http://localhost:3000\"\n"
	t += "\n"
	t += "# -- Services --\n"
	t += fmt.Sprintf("[services.%s]\n", serviceName)
	t += fmt.Sprintf("command = %q\n", serviceCommand)
	t += "# port = 3000\n"
	t += "# path = \"./\"\n"
	t += "# depends_on = [\"postgres\"]\n"
	t += "#\n"
	t += fmt.Sprintf("# env_file = \".env.%s\"  # Per-service .env file\n", serviceName)
	t += "#\n"
	t += fmt.Sprintf("# [services.%s.env]\n", serviceName)
	t += "# DATABASE_URL = \"postgres://user:${DB_PASS}@localhost:{{ docker.postgres.port }}/mydb\"\n"
	t += "# KUBECONFIG = \"{{ cluster.kubeconfig }}\"  # when service needs k3d access\n"
	t += "#\n"
	t += fmt.Sprintf("# [services.%s.restart]\n", serviceName)
	t += "# policy = \"on-failure\"\n"
	t += "# max_restarts = 10\n"
	t += "\n"
	t += "# -- Docker containers --\n"
	t += "# devrig manages Docker containers with health checks, init scripts, and volumes.\n"
	t += "#\n"
	t += "# [docker.postgres]\n"
	t += "# image = \"postgres:16-alpine\"\n"
	t += "# port = 5432\n"
	t += "# volumes = [\"pgdata:/var/lib/postgresql/data\"]\n"
	t += "# ready_check = { type = \"pg_isready\" }\n"
	t += fmt.Sprintf("# init = [\"CREATE DATABASE %s;\"]\n", projectName)
	t += "#\n"
	t += "# [docker.postgres.env]\n"
	t += "# POSTGRES_USER = \"devrig\"\n"
	t += "# POSTGRES_PASSWORD = \"devrig\"\n"
	t += "#\n"
	t += "# [docker.redis]\n"
	t += "# image = \"redis:7-alpine\"\n"
	t += "# port = 6379\n"
	t += "# ready_check = { type = \"cmd\", command = \"redis-cli ping\", expect = \"PONG\" }\n"
	t += "\n"
	t += "# -- External tools (k3d / kubectl / helm) --\n"
	t += "# By default devrig fetches pinned, checksum-verified copies on demand\n"
	t += "# into ~/.devrig/bin (run `devrig deps list`). Prefer your own PATH copies\n"
	t += "# with prefer = \"system\", or point at specific binaries.\n"
	t += "#\n"
	t += "# [tools]\n"
	t += "# prefer = \"vendored\"   # or \"system\"\n"
	t += "# kubectl = \"/usr/local/bin/kubectl\"\n"
	return t
}
