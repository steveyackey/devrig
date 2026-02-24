---
name: devrig
description: Manage and debug a devrig development environment. Use when the user asks about service health, errors, performance, traces, logs, metrics, debugging, project setup, configuration, starting/stopping services, or anything related to their local dev environment managed by devrig.
allowed-tools:
  - Bash(devrig *)
  - Read(devrig.toml)
  - Edit(devrig.toml)
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

## Configuration

Config file: `devrig.toml` (found by walking up from cwd). Override with `-f <path>`.

**Full field reference**: See [reference/configuration.md](reference/configuration.md)

### Common Examples

**Docker containers (postgres + redis):**

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

**Docker with bind mounts (mount host directories):**

```toml
[docker.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = [
    "pgdata:/var/lib/postgresql/data",          # named volume (managed by devrig)
    "./init-scripts:/docker-entrypoint-initdb.d", # bind mount (host dir)
]
ready_check = { type = "pg_isready" }
[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
```

Bind mounts use `/absolute`, `./relative`, or `../parent` paths as the source. No Docker volume is created — the host path is mounted directly.

**Docker with custom command/entrypoint:**

```toml
[docker.redis]
image = "redis:7-alpine"
port = 6379
command = ["redis-server", "--appendonly", "yes"]
ready_check = { type = "cmd", command = "redis-cli ping", expect = "PONG" }

[docker.worker]
image = "python:3.12-slim"
entrypoint = ["python", "-u"]
command = ["worker.py", "--verbose"]
```

Both `command` and `entrypoint` accept a string or a list of strings.

**Service connected to docker containers:**

```toml
[services.api]
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres", "redis"]

[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
REDIS_URL = "redis://localhost:{{ docker.redis.port }}"
```

**Dashboard with custom OTel settings:**

```toml
[dashboard]
port = 4000

[dashboard.otel]
grpc_port = 4317
http_port = 4318
retention = "2h"
```

**Build-only cluster images (for Jobs, CronJobs, init containers):**

```toml
[cluster.image.job-runner]
context = "./tools/job-runner"
# dockerfile = "Dockerfile"   # optional, defaults to Dockerfile
watch = true

[cluster.deploy.api]
context = "./services/api"
manifests = "k8s/api"
depends_on = ["job-runner"]   # ensures image exists before deploy
```

### Environment Variables

- `$VAR` / `${VAR}` expands from `.env` files or host environment
- `{{ docker.postgres.port }}` templates reference other services' ports
- `$$` for a literal `$`
- Every service receives `DEVRIG_<NAME>_HOST`, `DEVRIG_<NAME>_PORT`, `DEVRIG_<NAME>_URL` for all other services
- When dashboard is enabled, every service gets `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, `DEVRIG_DASHBOARD_URL`

## Workflows

### Setting Up a New Project

1. `devrig init` — generates starter `devrig.toml` with detected project type
2. Edit `devrig.toml` — add docker containers, dashboard, env vars as needed
3. `devrig validate` — check for errors before starting
4. `devrig start` — launch everything

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

## Docker vs Compose

- **`[docker.*]`** — devrig fully manages the container: pull, start, health-check, init scripts, volumes, teardown. Recommended for most infrastructure.
- **`[compose]`** — delegates to an existing `docker-compose.yml`. Good when you already have a working compose file and want to integrate into devrig's dependency graph.
- Compose is a stepping stone; migrate to `[docker.*]` blocks for full lifecycle control.

## Tips

- Use `devrig env <service>` to see exactly what env vars a service receives
- Trace IDs can be passed as full hex strings or prefixes
- Use `jq` for filtering: `devrig query traces --format jsonl | jq 'select(.has_error)'`
- The dashboard UI URL is shown in `devrig ps` output
- Telemetry is in-memory with configurable retention (default 1h)
- `devrig logs -F` for live tailing, `devrig query logs` for OTel-collected logs
- Output formats: `--format table` (human), `--format json` (pretty), `--format jsonl` (pipe to jq)
