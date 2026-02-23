---
name: devrig
description: Manage and debug a devrig development environment. Use when the user asks about service health, errors, performance, traces, logs, metrics, debugging, project setup, configuration, starting/stopping services, or anything related to their local dev environment managed by devrig.
allowed-tools:
  - Bash(devrig *)
  - Read(devrig.toml)
  - Edit(devrig.toml)
  - Read(docs/guides/configuration.md)
---

# devrig — Local Development Environment

You have access to devrig, a local development orchestrator with built-in OpenTelemetry collection. Use the commands and config reference below to help the user set up, manage, and debug their environment.

## Commands

### Project Setup

```bash
devrig init                        # Generate starter devrig.toml (detects project type)
devrig validate                    # Validate config with rich diagnostics
devrig validate -f devrig.alt.toml # Validate a specific config file
devrig doctor                      # Check dependencies (Docker, k3d, kubectl, etc.)
devrig skill install               # Install this skill to project .claude/skills/
devrig skill install --global      # Install to ~/.claude/skills/
```

### Service Lifecycle

```bash
devrig start                       # Start all services + docker + dashboard
devrig start api web               # Start named services + transitive deps
devrig start -f devrig.alt.toml    # Use a different config file
devrig stop                        # Stop services, preserve state
devrig delete                      # Stop + remove containers, volumes, networks, state
```

### Status & Inspection

```bash
devrig ps                          # Show running services with ports and status
devrig ps --all                    # Show all devrig instances across projects
devrig env <service>               # Show resolved env vars for a service
devrig exec <docker> -- <cmd>      # Execute command inside a docker container
devrig reset <docker>              # Clear init-completed flag (re-runs init scripts)
```

### Logs

```bash
devrig logs                        # All logs
devrig logs api web                # Filter to specific services
devrig logs --tail 100             # Last 100 lines
devrig logs --since 5m             # Last 5 minutes
devrig logs --grep "ERROR"         # Lines matching regex
devrig logs --exclude "health"     # Exclude lines matching regex
devrig logs --level warn           # Minimum log level
devrig logs --format json          # Output as JSONL
devrig logs -F                     # Follow (live tail)
devrig logs -o logs.txt            # Write to file
devrig logs -t                     # Show timestamps
```

### Telemetry Queries

```bash
# Traces
devrig query traces --service <name> --status <ok|error> --min-duration <ms> --limit <n> --format <table|json|jsonl>
devrig query trace <trace-id>                        # Span waterfall for a trace
devrig query related <trace-id>                      # Logs + metrics for a trace

# Logs (from OTel collector)
devrig query logs --service <name> --level <trace|debug|info|warn|error|fatal> --search <text> --trace-id <id> --limit <n>

# Metrics
devrig query metrics --name <metric> --service <name> --limit <n>

# Status
devrig query status                                  # OTel collector summary
```

### Cluster (k3d)

```bash
devrig cluster create              # Create cluster, build images, deploy manifests
devrig cluster delete              # Tear down cluster + registry
devrig cluster kubeconfig          # Print kubeconfig path
devrig kubectl get pods            # Proxy kubectl with devrig's kubeconfig
devrig k logs -f deploy/api        # Short alias for kubectl
```

### Other

```bash
devrig update                      # Self-update to latest version
devrig completions bash            # Generate shell completions (bash/zsh/fish/elvish/powershell)
```

## Configuration Reference

Config file: `devrig.toml` (found by walking up from cwd). Override with `-f <path>`.

Full reference: `docs/guides/configuration.md`

### `[project]` (required)

| Field      | Type   | Required | Description                        |
|------------|--------|----------|------------------------------------|
| `name`     | string | Yes      | Project name for display and slug  |
| `env_file` | string | No       | Path to project-level `.env` file  |

### `[env]`

Global env vars passed to every service. Per-service env overrides these.

```toml
[env]
RUST_LOG = "debug"
NODE_ENV = "development"
```

### `[services.*]`

| Field        | Type               | Required | Default      | Description                                  |
|--------------|--------------------|----------|--------------|----------------------------------------------|
| `command`    | string             | Yes      | --           | Shell command (via `sh -c`)                  |
| `path`       | string             | No       | config dir   | Working directory relative to config file    |
| `port`       | int or `"auto"`    | No       | (none)       | Port the service listens on                  |
| `env`        | map                | No       | `{}`         | Service-specific env vars                    |
| `env_file`   | string             | No       | (none)       | Per-service `.env` file path                 |
| `depends_on` | list               | No       | `[]`         | Services/docker/compose to start before this |

**Port values:** `3000` (fixed, verified available), `"auto"` (ephemeral, sticky across restarts), omitted (no management). When set, `PORT` env var is injected.

**Restart config** (`[services.<name>.restart]`):

| Field                  | Type    | Default      | Description                    |
|------------------------|---------|--------------|--------------------------------|
| `policy`               | string  | `on-failure` | `always`, `on-failure`, `never`|
| `max_restarts`         | int     | `10`         | Max restarts during runtime    |
| `startup_max_restarts` | int     | `3`          | Max restarts in startup phase  |
| `startup_grace_ms`     | int     | `2000`       | Startup phase duration (ms)    |
| `initial_delay_ms`     | int     | `500`        | Initial backoff delay (ms)     |
| `max_delay_ms`         | int     | `30000`      | Max backoff delay (ms)         |

### `[docker.*]`

| Field         | Type               | Required | Default | Description                              |
|---------------|--------------------|----------|---------|------------------------------------------|
| `image`         | string             | Yes      | --      | Docker image                             |
| `port`          | int or `"auto"`    | No       | (none)  | Single port mapping (host:container)     |
| `ports`         | map                | No       | `{}`    | Named port mappings (multi-port)         |
| `env`           | map                | No       | `{}`    | Container env vars                       |
| `volumes`       | list               | No       | `[]`    | Volume mounts (`"name:/path"`)           |
| `ready_check`   | table              | No       | (none)  | Health check config                      |
| `init`          | list               | No       | `[]`    | SQL/commands after first ready           |
| `depends_on`    | list               | No       | `[]`    | Other docker/compose dependencies        |
| `registry_auth` | table              | No       | (none)  | Private registry credentials (`username`, `password`) |

**Ready check types:**

| Type         | Runs     | Description                                 |
|--------------|----------|---------------------------------------------|
| `pg_isready` | container| `pg_isready -h localhost -q -t 2` (30s)     |
| `cmd`        | container| Custom command; optional `expect` string    |
| `http`       | host     | GET request, checks for 2xx (30s)           |
| `tcp`        | host     | TCP connection to host port (30s)           |
| `log`        | container| Stream logs, match pattern (60s)            |

```toml
ready_check = { type = "pg_isready" }
ready_check = { type = "cmd", command = "redis-cli ping", expect = "PONG" }
ready_check = { type = "http", url = "http://localhost:9000/health" }
ready_check = { type = "tcp" }
[docker.es.ready_check]
type = "log"
match = "started"
```

### `[dashboard]`

| Field     | Type    | Default | Description                         |
|-----------|---------|---------|-------------------------------------|
| `port`    | int     | `4000`  | Dashboard web UI and API port       |
| `enabled` | bool    | `true`  | Whether to start the dashboard      |

### `[dashboard.otel]`

| Field          | Type    | Default  | Description                       |
|----------------|---------|----------|-----------------------------------|
| `grpc_port`    | int     | `4317`   | OTLP gRPC receiver port           |
| `http_port`    | int     | `4318`   | OTLP HTTP receiver port           |
| `trace_buffer` | int     | `10000`  | Max spans in memory                |
| `metric_buffer`| int     | `50000`  | Max metric data points             |
| `log_buffer`   | int     | `100000` | Max log records                    |
| `retention`    | string  | `"1h"`   | Retention duration (e.g. `"2h30m"`)|

### `[compose]`

| Field          | Type    | Required | Default | Description                     |
|----------------|---------|----------|---------|---------------------------------|
| `file`         | string  | Yes      | --      | Path to docker-compose.yml      |
| `services`     | list    | No       | `[]`    | Services to start (auto-discovered if empty) |
| `env_file`     | string  | No       | (none)  | Env file for compose            |
| `ready_checks` | map     | No       | `{}`    | Ready checks for compose services|

### `[cluster]`

| Field      | Type    | Default         | Description                    |
|------------|---------|-----------------|--------------------------------|
| `name`     | string  | `devrig-{slug}` | k3d cluster name               |
| `agents`   | int     | `1`             | Number of agent nodes          |
| `ports`    | list    | `[]`            | Host-to-cluster port mappings  |
| `registry` | bool    | `true`          | Create local container registry|

### `[[cluster.registries]]`

Private registry auth for cluster image pulls. Each entry generates k3d `registries.yaml`.

| Field      | Type   | Required | Description              |
|------------|--------|----------|--------------------------|
| `url`      | string | Yes      | Registry hostname        |
| `username` | string | Yes      | Auth username            |
| `password` | string | Yes      | Auth password            |

```toml
[[cluster.registries]]
url = "ghcr.io"
username = "$REGISTRY_USER"
password = "$REGISTRY_TOKEN"
```

### `[cluster.deploy.*]`

| Field        | Type    | Required | Default      | Description                         |
|--------------|---------|----------|--------------|-------------------------------------|
| `context`    | string  | Yes      | --           | Docker build context dir            |
| `dockerfile` | string  | No       | `Dockerfile` | Dockerfile path relative to context |
| `manifests`  | list    | Yes      | --           | K8s manifest files to apply         |
| `watch`      | bool    | No       | `false`      | Auto-rebuild on file changes        |
| `depends_on` | list    | No       | `[]`         | Docker/deploy dependencies          |

### `[cluster.addons.*]`

Types: `helm`, `manifest`, `kustomize`. All support `namespace` and `port_forward`.

Helm: `chart`, `repo` (required), `version`, `values`
Manifest: `path` (required)
Kustomize: `path` (required)

```toml
[cluster.addons.traefik]
type = "helm"
chart = "traefik/traefik"
repo = "https://traefik.github.io/charts"
namespace = "traefik"
version = "26.0.0"
[cluster.addons.traefik.port_forward]
8080 = "svc/traefik:80"
```

### `[network]`

| Field  | Type   | Default            | Description           |
|--------|--------|--------------------|-----------------------|
| `name` | string | `devrig-{slug}-net`| Custom Docker network |

### Environment Variable Expansion

Any env value can reference host or `.env` file variables with `$VAR` or `${VAR}`. Use `$$` for a literal `$`. Expansion runs before template interpolation (`{{ }}`), so both can be combined.

**Lookup order:** `.env` file values → host process environment.

```toml
[project]
env_file = ".env"           # Load shared secrets

[env]
SECRET_KEY = "$MY_SECRET_KEY"

[services.api]
env_file = ".env.api"       # Per-service .env
[services.api.env]
DATABASE_URL = "postgres://user:${DB_PASS}@localhost:{{ docker.postgres.port }}/mydb"
```

**Supported expansion locations:** `[env]`, `[services.*.env]`, `[docker.*.env]`, `docker.*.image`, `docker.*.registry_auth.*`, `cluster.registries.*`.

**Secret masking:** `devrig env <service>` masks expanded secrets with `****`.

### Template Expressions

Service env values support `{{ dotted.path }}` templates:

| Variable                        | Example value |
|---------------------------------|---------------|
| `project.name`                  | `myapp`       |
| `services.<name>.port`          | `3000`        |
| `docker.<name>.port`            | `5432`        |
| `docker.<name>.ports.<portname>`| `1025`        |
| `compose.<name>.port`           | `6379`        |
| `cluster.name`                  | `myapp-dev`   |

```toml
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/mydb"
```

### Auto-injected `DEVRIG_*` Variables

Every service receives discovery vars for all other services and docker containers:

| Variable                         | Example                                     |
|----------------------------------|---------------------------------------------|
| `DEVRIG_<NAME>_HOST`             | `localhost`                                 |
| `DEVRIG_<NAME>_PORT`             | `5432`                                      |
| `DEVRIG_<NAME>_URL`              | `postgres://user:pass@localhost:5432`        |
| `DEVRIG_<NAME>_PORT_<PORTNAME>`  | `1025` (for named ports)                    |

### OTEL Auto-injection

When dashboard is enabled, every service gets:

| Variable                         | Description                           |
|----------------------------------|---------------------------------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT`   | OTLP gRPC endpoint (`http://localhost:4317`) |
| `OTEL_SERVICE_NAME`              | Service name from config              |
| `DEVRIG_DASHBOARD_URL`           | Dashboard URL (`http://localhost:4000`)|

## Workflows

### Setting Up a New Project

1. `devrig init` — generates starter `devrig.toml` with detected project type
2. Edit `devrig.toml` — add docker containers, dashboard, env vars as needed
3. `devrig validate` — check for errors before starting
4. `devrig start` — launch everything

### Adding Docker Containers

```toml
[docker.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
ready_check = { type = "pg_isready" }
[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"

[docker.redis]
image = "redis:7-alpine"
port = 6379
ready_check = { type = "cmd", command = "redis-cli ping", expect = "PONG" }
```

### Enabling the Dashboard

Add a `[dashboard]` section (even empty works):

```toml
[dashboard]
```

To customize OTel settings:

```toml
[dashboard]
port = 4000

[dashboard.otel]
grpc_port = 4317
http_port = 4318
retention = "2h"
```

### Using Private Docker Registries

```toml
[docker.my-app]
image = "ghcr.io/org/app:latest"
registry_auth = { username = "$REGISTRY_USER", password = "$REGISTRY_TOKEN" }
```

### Connecting Services to Docker Containers

```toml
[services.api]
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres", "redis"]

[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
REDIS_URL = "redis://localhost:{{ docker.redis.port }}"
```

### Debugging Performance Issues

1. Find slow traces: `devrig query traces --min-duration 500 --limit 10`
2. Inspect the trace: `devrig query trace <trace-id>`
3. Check related telemetry: `devrig query related <trace-id>`
4. Search error logs: `devrig query logs --service <name> --level error --limit 20`

### Investigating Errors

1. Find error traces: `devrig query traces --status error --limit 10`
2. Get trace detail: `devrig query trace <trace-id>`
3. Search error logs: `devrig query logs --level error --limit 30`
4. Narrow to a service: `devrig query logs --service <name> --level warn --search "timeout"`
5. Cross-reference: `devrig query related <trace-id>`

### Checking System Health

1. OTel status: `devrig query status`
2. Check metrics: `devrig query metrics --limit 50`
3. Look for warnings: `devrig query logs --level warn --limit 30`
4. Service-specific: `devrig query traces --service <name> --limit 10`

## Tips

- Use `devrig env <service>` to see exactly what env vars a service receives
- Trace IDs can be passed as full hex strings or prefixes
- Use `jq` for filtering: `devrig query traces --format jsonl | jq 'select(.has_error)'`
- The dashboard UI URL is shown in `devrig ps` output
- Telemetry is in-memory with configurable retention (default 1h)
- `devrig logs -F` for live tailing, `devrig query logs` for OTel-collected logs
- Output formats: `--format table` (human), `--format json` (pretty), `--format jsonl` (pipe to jq)
