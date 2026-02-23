# devrig — Product Requirements Document

*A local development orchestrator for services, containers, and Kubernetes — inspired by Aspire, built for the Docker/Rust/k3d ecosystem.*

---

## Problem

Setting up a local development environment for a multi-service project is painful. You need to:

- Run multiple services with hot reload (Rust backend, SolidJS frontend, maybe a worker)
- Spin up infrastructure (Postgres, Redis, etc.) as containers
- Optionally run things in a local Kubernetes cluster for prod-parity
- Wire environment variables, ports, and service discovery between all of it
- Remember the right incantation of `docker compose up`, `k3d cluster create`, `cargo watch`, `pnpm dev`...

Aspire solves this elegantly for the .NET world. **devrig** does it for everyone else — with a Rust CLI, Docker as the container runtime, and k3d when you need Kubernetes.

## Design Principles

1. **Nothing by default.** A bare `devrig.toml` does nothing. You opt in to every capability.
2. **Config is the source of truth.** One file describes your entire dev environment. No hidden state.
3. **Three commands.** `devrig start`, `devrig stop`, `devrig delete`. That's the core loop.
4. **Language-agnostic services.** devrig doesn't care what your services are written in. It runs commands.
5. **Progressive complexity.** Start with one service. Add Postgres when you need it. Add k3d later. Each step is one config block.
6. **No lock-in.** devrig wraps tools you already use (Docker, k3d, cargo-watch, etc.). Eject anytime.

---

## Configuration

All configuration lives in a single `devrig.toml` at the project root (or wherever you run devrig from). The presence or absence of sections determines what devrig manages. Use `devrig start -f devrig.staging.toml` to switch between config files — this replaces the need for profiles or environments.

### Full annotated example

```toml
# devrig.toml

# ------------------------------------------------------------------
# Project metadata
# ------------------------------------------------------------------
[project]
name = "myapp"                    # Used as prefix for containers, networks, cluster name

# ------------------------------------------------------------------
# Services: local processes with hot reload
# ------------------------------------------------------------------
# Each [services.<name>] block is a process devrig manages.
# devrig starts the command, passes env vars, and streams logs.

[services.api]
path = "./services/api"           # Working directory (relative to this file)
command = "cargo watch -x run"    # The hot-reload command
port = 3000                       # Port this service listens on (explicit)
env = { DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp" }
depends_on = ["postgres"]         # Wait for these before starting

[services.web]
path = "./apps/web"
command = "pnpm dev"
port = 5173
env = { VITE_API_URL = "http://localhost:{{ services.api.port }}" }
depends_on = ["api"]

[services.worker]
path = "./services/worker"
command = "cargo watch -x run"
port = "auto"                     # Auto-assign an available port
env = { DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp" }
depends_on = ["postgres"]

# ------------------------------------------------------------------
# Docker containers managed by devrig
# ------------------------------------------------------------------
# Each [docker.<name>] block is a Docker container devrig runs.
# devrig handles pulling images, creating volumes, health checks, etc.

[docker.postgres]
image = "postgres:16"
port = 5432                       # Host port (explicit). Use "auto" to auto-assign.
env = { POSTGRES_USER = "devrig", POSTGRES_PASSWORD = "devrig" }
volumes = ["pgdata:/var/lib/postgresql/data"]
ready_check = { type = "pg_isready" }   # Built-in health check strategies
init = [
  "CREATE DATABASE myapp;",       # SQL to run on first start
]

[docker.redis]
image = "redis:7-alpine"
port = "auto"                     # Auto-assign an available host port
ready_check = { type = "cmd", command = "redis-cli ping" }

[docker.mailpit]
image = "axllent/mailpit:latest"
ports = { smtp = 1025, ui = 8025 }

# ------------------------------------------------------------------
# Cluster: optional k3d Kubernetes cluster
# ------------------------------------------------------------------
# If this section is present, devrig creates a k3d cluster.
# Services can be deployed into it instead of running as local processes.

[cluster]
name = "{{ project.name }}-dev"   # Defaults to project name + "-dev"
agents = 1                        # Number of agent nodes
ports = ["8080:80@loadbalancer"]  # Port mappings exposed from cluster
registry = true                   # Spin up a local registry for images
# kubeconfig is always isolated in .devrig/kubeconfig — never touches ~/.kube/config
# Use `devrig kubectl ...` or `devrig k ...` to interact with the cluster

# Deploy specific services into the cluster instead of running locally
[cluster.deploy.api]
context = "./services/api"        # Docker build context
dockerfile = "Dockerfile.dev"     # Dockerfile to use
manifests = "./k8s/api/"          # Kubernetes manifests to apply
watch = true                      # Rebuild + redeploy on file changes

# ------------------------------------------------------------------
# Dashboard + Observability
# ------------------------------------------------------------------
# If present, devrig runs a local web dashboard with built-in OTel collector.

[dashboard]
port = 4000

[dashboard.otel]
grpc_port = 4317
http_port = 4318
trace_buffer = 10000
metric_buffer = 50000
log_buffer = 100000
retention = "1h"

# ------------------------------------------------------------------
# Environment: shared env vars injected into all services
# ------------------------------------------------------------------
[env]
ENVIRONMENT = "development"
LOG_LEVEL = "debug"

# ------------------------------------------------------------------
# Networking
# ------------------------------------------------------------------
[network]
# Network name is auto-generated from project slug (e.g., devrig-myapp-a3f1c9e2-net)
# All docker containers and cluster share this network.
# Services get env vars pointing to the right addresses.
```

### Configuration design decisions

**Why TOML?** It's the Rust ecosystem standard (`Cargo.toml`). It handles nested config well without YAML's footguns. And it's easy to read.

**Template expressions** like `{{ project.name }}`, `{{ docker.postgres.port }}`, and `{{ services.api.port }}` allow cross-referencing within the config. Port references resolve to the actual assigned value (including auto-assigned ports). Only simple variable interpolation — no logic, no loops.

**`depends_on` is dependency ordering, not health gating (by default).** If you need health gating, the dependency target needs a `ready_check`. devrig waits for `ready_check` to pass before starting dependents.

**`init` blocks run once.** devrig tracks whether init has run via a state file (`.devrig/state.json`). Re-running init requires `devrig reset <name>` or `devrig delete`.

---

## Multi-Instance Isolation

Developers commonly work on multiple projects simultaneously. Two `devrig start` commands in different terminals, for different projects, must never interfere with each other. devrig achieves this by scoping every resource it creates to a **project identity** derived from the project name and config file location.

### Project identity

Every devrig instance has a unique identity composed of:

```
project_id = hash(canonical_path_of_devrig_toml)[:8]
project_slug = "{project.name}-{project_id}"
```

For example, if `project.name = "myapp"` and the config lives at `/home/steve/code/myapp/devrig.toml`, the slug might be `myapp-a3f1c9e2`. This slug prefixes every Docker resource devrig creates. The hash ensures that even two projects with the same `project.name` on different paths don't collide.

### Resource naming

All Docker resources use the project slug as a namespace:

| Resource | Naming pattern | Example |
|---|---|---|
| Containers | `devrig-{slug}-{service}` | `devrig-myapp-a3f1c9e2-postgres` |
| Networks | `devrig-{slug}-net` | `devrig-myapp-a3f1c9e2-net` |
| Volumes | `devrig-{slug}-{volume}` | `devrig-myapp-a3f1c9e2-pgdata` |
| k3d cluster | `devrig-{slug}` | `devrig-myapp-a3f1c9e2` |
| Docker labels | `devrig.project={slug}` | Used for discovery and cleanup |

Every resource also gets a `devrig.project` Docker label so `devrig stop` and `devrig delete` can find exactly the resources belonging to this instance — even if `.devrig/state.json` is lost.

### Directory-aware commands

devrig determines which project it's operating on by looking for `devrig.toml` (or the file specified by `-f`) starting from the current working directory and walking up. This means:

```bash
cd ~/code/myapp && devrig stop     # Stops myapp
cd ~/code/other && devrig stop     # Stops other — completely independent
```

If no `devrig.toml` is found in the directory tree, devrig exits with a clear error: `No devrig.toml found in /home/steve/code/foo or any parent directory`.

### Port collision detection

When two projects try to bind the same host port, devrig detects the conflict at startup:

```
  devrig ⚡ otherproject

  ✗ Port 5432 is already in use (by devrig-myapp-a3f1c9e2-postgres)
    → Change [docker.postgres] port in devrig.toml, or stop the other project first

  Startup failed: port conflict
```

devrig checks for port availability before starting any container or process. The error message identifies which devrig project owns the conflicting port (via Docker label lookup) so the developer knows exactly what to do.

### `devrig ps --all`

To see all running devrig instances across all projects:

```bash
devrig ps --all
```

```
  Project       Config                              Services  Docker  Cluster  Status
  myapp         ~/code/myapp/devrig.toml            3         2      yes      ● running
  otherproject  ~/code/other/devrig.toml             1         1      no       ● running
  experiment    ~/code/exp/devrig.minimal.toml       1         0      no       ● stopped
```

This queries Docker labels to discover all devrig-managed resources on the machine, regardless of which directory you're in.

### State file scoping

The `.devrig/` state directory is per-project (it lives next to the `devrig.toml` file), so state is inherently scoped. Two projects never share a `.devrig/` directory.

### Cleanup safety

`devrig delete` only removes resources with the matching `devrig.project={slug}` label. It never touches resources from other projects. The label-based discovery means cleanup works even if `.devrig/state.json` is corrupted or missing.

---

## CLI

### Global flags

```
-f <file>                 # Use a specific config file (default: devrig.toml)
                          # e.g., devrig start -f devrig.minimal.toml
                          # Use separate files instead of profiles
```

### Core commands

```
devrig start              # Start everything defined in devrig.toml
devrig start -f devrig.minimal.toml  # Start from a different config
devrig stop               # Gracefully stop everything (preserves state/volumes)
devrig delete             # Stop + remove containers, volumes, cluster, network
```

### Service management

```
devrig start api web      # Start only specific services (+ their dependencies)
devrig stop api           # Stop a specific service
devrig restart api        # Stop + start a specific service
devrig logs               # Unified, color-coded log stream (all services)
devrig logs api           # Logs for a specific service
devrig logs --follow      # Tail mode (default)
```

### Docker container management

```
devrig ps                 # Show status of all services and docker containers for this project
devrig ps --all           # Show all running devrig instances across all projects
devrig exec postgres      # Shell into a docker container
devrig reset postgres     # Re-run init scripts for a docker component
devrig env api            # Print all resolved env vars for a service
devrig env api DEVRIG_POSTGRES_URL  # Print a specific var
devrig env --export api   # Print as export statements (for eval)
```

### Cluster management

```
devrig cluster create     # Manually create cluster (start does this too)
devrig cluster delete     # Tear down just the cluster
devrig cluster kubeconfig # Print path to devrig's isolated kubeconfig
devrig kubectl ...        # Proxy to kubectl with KUBECONFIG set to devrig's config
devrig k ...              # Short alias for devrig kubectl
devrig k get pods         # Example: list pods in the devrig cluster
devrig k logs deploy/api  # Example: tail logs from a cluster-deployed service
```

### Utilities

```
devrig init               # Generate a starter devrig.toml interactively
devrig validate           # Check devrig.toml for errors
devrig doctor             # Check that dependencies (Docker, k3d, etc.) are installed
devrig skill install      # Install Claude Code skill to .claude/skills/ in current project
devrig skill install --global  # Install Claude Code skill to ~/.claude/skills/
```

---

## Architecture

### Tech stack

| Component | Technology |
|---|---|
| CLI framework | `clap` |
| Async runtime | `tokio` |
| Config parsing | `serde` + `toml` |
| Docker interaction | `bollard` (Docker Engine API client) |
| k3d interaction | Shell out to `k3d` CLI |
| Process management | `tokio::process` |
| Log multiplexing | `tokio::select!` over stdout/stderr streams |
| Web dashboard + OTel UI | Axum + SolidJS + [Solid UI](https://www.solid-ui.com/) |
| OTel collector | In-process OTLP receiver (gRPC + HTTP) |
| OTel storage | In-memory ring buffers with configurable retention |
| Template interpolation | Custom lightweight resolver (no full template engine) |

### Runtime model

```
devrig start
  │
  ├─ Parse devrig.toml
  ├─ Resolve dependency graph (topological sort)
  ├─ Create Docker network (if docker containers or cluster present)
  │
  ├─ Phase 1: Docker Containers
  │   ├─ Pull images (parallel)
  │   ├─ Start containers (respecting depends_on)
  │   ├─ Run ready_checks (poll with backoff)
  │   └─ Run init scripts (if first time)
  │
  ├─ Phase 2: Cluster (if configured)
  │   ├─ k3d cluster create (if not exists)
  │   ├─ Connect cluster network to devrig network
  │   ├─ Build + push images to local registry
  │   └─ Apply manifests
  │
  ├─ Phase 3: Services
  │   ├─ Inject env vars (global + service-specific + auto-generated)
  │   ├─ Spawn processes (respecting depends_on)
  │   └─ Begin log multiplexing
  │
  ├─ Phase 4: Dashboard + OTel collector
  │   ├─ Start Axum server on available port
  │   ├─ Start OTLP receiver (gRPC :4317, HTTP :4318)
  │   ├─ Inject OTEL_EXPORTER_OTLP_ENDPOINT into all services
  │   └─ Initialize in-memory ring buffers for traces/metrics/logs
  │
  ├─ Print startup summary
  │   ├─ Service URLs (http://localhost:<port> for each service)
  │   ├─ Docker endpoints (postgres://..., redis://...)
  │   ├─ Dashboard URL (http://localhost:<dashboard_port>)
  │   └─ OTel collector endpoints
  │
  └─ Enter watch mode
      ├─ Stream unified logs
      ├─ Monitor process health (restart on crash with backoff)
      └─ Watch for cluster.deploy file changes (rebuild + redeploy)
```

### Startup output

When `devrig start` finishes bringing everything up, it prints a clear summary:

```
  devrig ⚡ myapp (a3f1c9e2)

  Services
    api       http://localhost:3000    ● running
    web       http://localhost:5173    ● running
    worker                             ● running

  Docker
    postgres  localhost:5432           ● ready
    redis     localhost:6379           ● ready
    mailpit   http://localhost:8025    ● ready

  Cluster     myapp-dev               ● ready
    Use: devrig k get pods

  Dashboard   http://localhost:4000
  OTel gRPC   localhost:4317
  OTel HTTP   localhost:4318

  Press Ctrl+C to stop all services
```

The dashboard port defaults to `4000` but is configurable via `[dashboard.port]`.

### State management

devrig keeps minimal state in `.devrig/` (gitignored):

```
.devrig/
  state.json        # Tracks which init scripts have run, container IDs, etc.
  kubeconfig        # Isolated kubeconfig for the k3d cluster (if cluster enabled)
  logs/             # Rotated log files (optional)
```

State is advisory, not critical. `devrig delete` wipes it all — including the kubeconfig. If state is corrupted or missing, devrig recovers by inspecting Docker/k3d directly.

**kubeconfig is always isolated.** devrig never reads or writes `~/.kube/config`. The k3d cluster's kubeconfig lives at `.devrig/kubeconfig` and is only used via `devrig kubectl` / `devrig k`, which sets `KUBECONFIG` automatically. On `devrig delete`, the kubeconfig file is simply removed along with the cluster — no merge/unmerge, no restore, no risk of polluting the user's global config.

### Service Discovery & Environment Injection

devrig automatically injects environment variables into every service so they can discover each other and all infrastructure — without hardcoding ports, hosts, or connection strings. This is the primary mechanism for service-to-service and service-to-docker communication.

#### Auto-generated variables

Every service and docker component gets a set of `DEVRIG_` prefixed env vars injected into **all** services:

```bash
# ─── Docker Containers ─────────────────────────────────────────
# For each [docker.<name>]:
DEVRIG_POSTGRES_HOST=localhost
DEVRIG_POSTGRES_PORT=5432
DEVRIG_POSTGRES_URL=postgres://devrig:devrig@localhost:5432  # Connection URL (when credentials are in env)

DEVRIG_REDIS_HOST=localhost
DEVRIG_REDIS_PORT=63221              # Resolved auto-assigned port

DEVRIG_MAILPIT_HOST=localhost
DEVRIG_MAILPIT_PORT_SMTP=1025       # Named ports from `ports = { smtp = 1025, ui = 8025 }`
DEVRIG_MAILPIT_PORT_UI=8025

# ─── Services ──────────────────────────────────────────────────
# For each [services.<name>]:
DEVRIG_API_HOST=localhost
DEVRIG_API_PORT=3000
DEVRIG_API_URL=http://localhost:3000

DEVRIG_WEB_HOST=localhost
DEVRIG_WEB_PORT=5173
DEVRIG_WEB_URL=http://localhost:5173

DEVRIG_WORKER_HOST=localhost
DEVRIG_WORKER_PORT=48832             # Auto-assigned port

# ─── Own service ───────────────────────────────────────────────
# Injected into the service's own process only:
PORT=3000                            # The service's own port (works with frameworks that read PORT)
HOST=localhost

# ─── Dashboard / OTel ─────────────────────────────────────────
DEVRIG_DASHBOARD_URL=http://localhost:4000
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
OTEL_SERVICE_NAME=api                # Set to the service's own name
```

**Every service can discover every other service and docker component just by reading env vars.** No config, no DNS, no consul. A service that needs to talk to postgres reads `DEVRIG_POSTGRES_URL`. A service that needs to call the API reads `DEVRIG_API_URL`. This works regardless of whether ports are fixed or auto-assigned.

#### URL generation rules

devrig generates `_URL` vars using these rules:

| Component | URL pattern | Example |
|---|---|---|
| Services with `port` | `http://localhost:{port}` | `DEVRIG_API_URL=http://localhost:3000` |
| Postgres docker | `postgres://{user}:{pass}@localhost:{port}` | `DEVRIG_POSTGRES_URL=postgres://devrig:devrig@localhost:5432` |
| Redis docker | `redis://localhost:{port}` | `DEVRIG_REDIS_URL=redis://localhost:63221` |
| Generic docker | `{host}:{port}` | `DEVRIG_MAILPIT_URL=localhost:1025` |
| Multi-port docker | No single URL; use `_PORT_<name>` vars | `DEVRIG_MAILPIT_PORT_SMTP=1025` |

For docker containers with credentials in `env` (like Postgres), devrig parses `POSTGRES_USER` and `POSTGRES_PASSWORD` to construct the URL. If credentials aren't present, the URL omits them.

#### Template expressions for config-time wiring

In addition to runtime env vars, template expressions let you wire things together in the TOML itself:

```toml
[services.api]
port = 3000
env = { DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp" }

[services.web]
env = { VITE_API_URL = "http://localhost:{{ services.api.port }}" }

[services.worker]
port = "auto"
env = {
  DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp",
  API_ENDPOINT = "http://localhost:{{ services.api.port }}",
  REDIS_URL = "redis://localhost:{{ docker.redis.port }}"
}
```

**When to use which:**

- **Template expressions** (`{{ }}`) — for explicit env vars in the config where you need control over the format (e.g., `DATABASE_URL` with a specific database name, custom path suffixes)
- **Auto-generated `DEVRIG_*` vars** — for everything else. Your code just reads `DEVRIG_POSTGRES_URL` and it works. Zero config.

Both resolve auto-assigned ports correctly. Template expressions are evaluated after all ports are assigned.

#### Automatic port selection

Any `port` field can be set to `"auto"` instead of a fixed number. devrig finds an available port at startup.

**How auto ports work:**

1. During startup, devrig scans for available ports (starting from a configurable range, default `10000–65000`)
2. The resolved port is stored in `.devrig/state.json` so it persists across `stop`/`start` (but not `delete`)
3. The resolved port is injected via all the mechanisms above: `DEVRIG_<NAME>_PORT`, `DEVRIG_<NAME>_URL`, `PORT` (for the service's own process), and template expression resolution
4. On next `devrig start` (without `delete`), the same port is reused if still available

**Startup output shows resolved ports:**

```
  Services
    api       http://localhost:3000    ● running
    web       http://localhost:5173    ● running
    worker    http://localhost:48832 (auto)  ● running

  Docker
    postgres  localhost:5432           ● ready
    redis     localhost:63221 (auto)   ● ready
    mailpit   localhost:1025 / http://localhost:8025  ● ready
```

#### Discovery from outside devrig

For scripts or tools that need to discover devrig-managed services:

```bash
# Print all resolved env vars for a service
devrig env api

# Print a specific var
devrig env api DEVRIG_POSTGRES_URL

# Print all vars (useful for shell scripts)
eval $(devrig env --export api)
```

This is also available via the REST API:

```
GET /api/env                    # All env vars for all services
GET /api/env/:service           # All env vars for a specific service
```

---

## Project Structure

```
devrig/
  Cargo.toml
  Cargo.lock
  src/
    main.rs                  # CLI entry point (clap)
    lib.rs                   # Core library, re-exports
    config/                  # TOML parsing, validation, template interpolation
    orchestrator/            # Dependency graph, phased startup/shutdown
    services/                # Local process management (spawn, watch, restart)
    docker/                  # Docker container lifecycle (bollard)
    cluster/                 # k3d cluster management, kubeconfig isolation
    compose/                 # docker-compose.yml interop
    otel/                    # OTLP receiver, ring buffers, query engine
    dashboard/               # Axum server, WebSocket, REST API
    query/                   # CLI query subcommands (traces, metrics, logs, status)
  dashboard-ui/              # SolidJS + Solid UI frontend
    src/
      index.tsx
      components/
      views/                 # Overview, Traces, Metrics, Logs
      lib/                   # WebSocket client, data hooks
    package.json
    vite.config.ts
  tests/
    unit/                    # Pure logic tests (no Docker)
    integration/             # Real Docker/k3d tests (feature-gated)
  e2e/
    dashboard/               # Agent-browser E2E tests for the UI
  skill/
    claude-code/             # Claude Code skill (shipped with devrig)
      SKILL.md               # Skill instructions for Claude
  docs/                      # Project documentation (committed to repo)
    architecture/
      overview.md            # High-level system architecture
      config-model.md        # Configuration design and parsing
      dependency-graph.md    # How depends_on and phased startup work
      multi-instance.md      # Project identity, resource naming, collision detection
      service-discovery.md   # Env var injection, URL generation, template expressions
      otel-storage.md        # Ring buffer design, retention, eviction
      kubeconfig-isolation.md # Why we never touch ~/.kube/config
    adr/                     # Architecture Decision Records
      001-toml-only.md
      002-no-profiles.md
      003-isolated-kubeconfig.md
      004-compose-interop.md
      005-traefik-over-nginx.md
      006-in-memory-otel.md
      007-agent-browser-testing.md
      008-multi-instance-isolation.md
    guides/
      getting-started.md     # Quickstart for new users
      configuration.md       # Full config reference with examples
      cluster-setup.md       # k3d cluster guide
      compose-migration.md   # Moving from docker-compose to native docker blocks
      contributing.md        # How to contribute, run tests, etc.
    api/
      rest-api.md            # Dashboard REST API reference
      query-cli.md           # devrig query command reference
  README.md                  # Project README (see below)
  .devrig/                   # Runtime state (gitignored)
    state.json
    kubeconfig
    logs/
  .gitignore                 # Includes .devrig/
```

### README

The `README.md` is the front door to devrig. It should sell the project in 10 seconds and get someone running in 60.

**Structure:**

1. **One-liner** — what devrig is, in one sentence
2. **Demo** — terminal recording (asciinema or gif) showing `devrig init` → `devrig start` → services running → dashboard open → `devrig delete`. 30 seconds max.
3. **Quickstart** — copy-pasteable commands to go from zero to running:
   ```bash
   cargo install devrig
   cd your-project
   devrig init            # generates devrig.toml interactively
   devrig start           # everything comes up
   ```
4. **Example `devrig.toml`** — the minimal annotated config (3 services, postgres, that's it). Not the full annotated example from the PRD — a stripped-down version that shows the core value.
5. **What devrig manages** — quick visual showing the three layers (services → docker → cluster) with a one-liner for each
6. **Key features** — brief list: service discovery via env vars, ready checks, hot reload, OTel dashboard, multi-instance isolation, compose interop, k3d cluster, Claude Code skill
7. **CLI reference** — the core commands with one-line descriptions, link to full docs
8. **Dashboard screenshot** — a single screenshot of the traces view with the waterfall
9. **Configuration reference** — link to `docs/guides/configuration.md`
10. **Contributing** — link to `docs/guides/contributing.md`
11. **License**

**Rules for the README:**
- No walls of text. Scannable in 30 seconds.
- Every code block must be copy-pasteable.
- Keep it under 200 lines. Detailed docs live in `docs/`.
- Update it with every milestone.

### Documentation strategy

**`docs/` is a first-class part of the repo.** Every architectural decision, every non-obvious design choice, every "why did we do it this way" gets documented here. This isn't afterthought documentation — it's written alongside the code.

**Architecture Decision Records (ADRs)** follow a simple format: context, decision, consequences. One file per decision. Numbered sequentially. Decisions already made (TOML only, no profiles, isolated kubeconfig, compose interop, Traefik over nginx, in-memory OTel, agent-browser testing, multi-instance isolation) each get an ADR in v0.1.

**Architecture docs** explain the internals for contributors. How the config model works, how the dependency graph resolves, how the OTel ring buffers evict data. Updated when the implementation changes.

**Guides** are user-facing. Getting started, full config reference, cluster setup, migrating from compose. These grow with each milestone.

**API docs** cover the REST API and `devrig query` CLI reference. Auto-generated where possible, hand-written where clarity matters.

**Rule: no feature merges without updating docs/.** If a PR adds a config option, the config reference gets updated. If a PR changes startup behavior, the architecture doc reflects it. Tests enforce the feature works; docs explain why it exists.

---

## Ready check strategies

Built-in strategies for common infrastructure:

| Type | Behavior |
|---|---|
| `pg_isready` | Runs `pg_isready` against the container |
| `cmd` | Runs an arbitrary command inside the container |
| `http` | Polls an HTTP endpoint for 2xx |
| `tcp` | Checks if a TCP port is accepting connections |
| `log` | Watches container logs for a specific string |

```toml
# Examples
ready_check = { type = "pg_isready" }
ready_check = { type = "http", url = "http://localhost:8025/api/v1/info" }
ready_check = { type = "tcp" }                    # Uses the container's exposed port
ready_check = { type = "log", match = "ready to accept connections" }
ready_check = { type = "cmd", command = "redis-cli ping", expect = "PONG" }
```

All ready checks use exponential backoff with a configurable timeout (default: 30s).

---

## Observability — Built-in OTel Dashboard

devrig includes a built-in OpenTelemetry collector and observability dashboard. No Jaeger, no Grafana, no Prometheus — just devrig.

### How it works

devrig runs an in-process OTLP receiver that accepts traces, metrics, and logs over both gRPC (`:4317`) and HTTP (`:4318`). All telemetry is stored in **in-memory ring buffers** with configurable size limits — no disk, no external database. When the buffer fills, the oldest data rolls off.

Services get `OTEL_EXPORTER_OTLP_ENDPOINT` injected automatically, so any OTel-instrumented service sends telemetry to devrig with zero config.

### Configuration

```toml
[dashboard]
port = 4000                          # Dashboard UI port
enabled = true                       # Can be disabled if you don't want it

[dashboard.otel]
grpc_port = 4317                     # OTLP gRPC receiver port
http_port = 4318                     # OTLP HTTP receiver port
trace_buffer = 10000                 # Max traces in memory
metric_buffer = 50000                # Max metric data points in memory
log_buffer = 100000                  # Max log records in memory
retention = "1h"                     # Drop data older than this (even if buffer not full)
```

All of this is optional. If `[dashboard]` is absent, no dashboard or OTel collector runs.

### Dashboard UI (SolidJS + Solid UI)

Built with [Solid UI](https://www.solid-ui.com/) for a polished, fast, accessible interface out of the box. The dashboard should feel like a first-class product — beautiful, snappy, zero jank. Solid UI provides the component primitives (tables, tabs, dialogs, command palette, etc.) so we focus on the data views, not reinventing UI widgets.

The web dashboard at `http://localhost:4000` has a left sidebar for navigation and four main views:

**Overview** — At-a-glance status of all services, infrastructure, and cluster. Health indicators, uptime, port mappings, quick-action buttons (restart, stop). This is the landing page.

**Traces** — Waterfall view of distributed traces across services. Click a trace to see spans, timing, attributes, and errors. Filter by service, status, duration, or search span names. Shows the most recent traces first. Span detail in a slide-over panel. Each span shows jump-to links: "View logs" (logs from this service during the span's time window) and "View metrics" (metrics from this service at this timestamp).

**Metrics** — Time-series charts for any metrics your services emit (request duration histograms, counters, gauges). Auto-discovers metric names — no configuration of dashboards needed. Select a metric, see a chart. Filter by service and label. Click any point on a chart to jump-to: "View traces" (traces from this service in the surrounding time window) or "View logs" (logs from the same time window).

**Logs** — Structured log viewer pulling from the OTLP log signal (not the process stdout — that's in `devrig logs`). Filter by severity, service, and search body text. Each log line with a trace ID shows a "View trace" link that jumps directly to the trace waterfall. Log lines without trace context still show "View traces" (traces from this service around the log timestamp) and "View metrics" (metrics at this timestamp).

All four views update in real-time via WebSocket. The UI is intentionally focused — this is a dev tool, not a production monitoring platform. Fast load, instant navigation, no spinners on cached data.

### Cross-telemetry navigation

The killer feature of devrig's dashboard is that traces, metrics, and logs are all in the same process, in the same memory. No query federation, no cross-service auth, no Grafana datasource config. This makes jump-to between telemetry sources instant and seamless.

**Every piece of telemetry links to related telemetry.** The connections:

```
Trace ←→ Logs       Via trace ID (exact match) or service + time window
Trace ←→ Metrics    Via service + span time window
Logs  ←→ Metrics    Via service + timestamp window
Logs  ←→ Traces     Via trace ID (if present) or service + time window
Metrics ←→ Traces   Via service + time window around the data point
Metrics ←→ Logs     Via service + time window around the data point
```

**How it works in the UI:**

- **Trace ID links** are the strongest connection. If a log line carries a trace ID, clicking "View trace" jumps directly to that exact trace. If a trace exists, its detail panel lists all correlated logs.
- **Time window links** are the fallback. When there's no trace ID, devrig uses the service name + a configurable time window (default: ±5 seconds) around the event to find related telemetry. This is approximate but almost always useful.
- **Metric chart click-to-explore.** Clicking a data point on a metric chart opens a context menu: "View traces around this time" and "View logs around this time". The time window centers on the clicked point.
- **URL-based deep links.** Every jump-to is a URL: `/traces?traceId=abc123`, `/logs?service=api&from=...&to=...`, `/metrics?name=http.duration&service=api&at=...`. These can be shared, bookmarked, or used by agents.

**Keyboard shortcuts for navigation:**

- `t` from any telemetry detail → jump to related traces
- `l` from any telemetry detail → jump to related logs
- `m` from any telemetry detail → jump to related metrics
- `Escape` → back to previous view (breadcrumb navigation)

### Design principles for the dashboard

- **Dark mode default** with light mode toggle. Developers live in dark mode.
- **Keyboard-first.** Cmd+K command palette for jumping between views, services, traces. Arrow keys in tables.
- **Dense but readable.** Show more data per screen. Monospace for IDs/timestamps, proportional for labels.
- **Zero config.** No dashboard setup, no metric configuration, no chart building. Everything auto-discovers.
- **Sub-second feel.** WebSocket push for new data. Optimistic UI. Virtual scrolling for large log/trace lists.

### Storage model

```
Ring Buffer (in-memory)
┌──────────────────────────────────────────┐
│ newest → ... → oldest │ ← evicted       │
└──────────────────────────────────────────┘
          ↑ max_size items
          ↑ max_age retention
```

Data is evicted when either the buffer is full or the retention window expires, whichever comes first. On `devrig stop`, all telemetry is lost (by design — this is ephemeral dev data). On `devrig start`, buffers are empty.

---

## Agentic CLI — Machine-readable observability endpoints

devrig exposes CLI subcommands that return structured JSON, designed to be consumed by AI coding agents (like Claude Code skills), scripts, and other tools. These mirror the dashboard but are optimized for programmatic access.

### Commands

```
devrig query traces                     # List recent traces (JSON)
devrig query traces --service api       # Filter by service
devrig query traces --status error      # Only errored traces
devrig query traces --min-duration 100ms
devrig query trace <trace-id>           # Full trace detail with all spans

devrig query metrics                    # List all known metric names
devrig query metrics --name http.server.duration
devrig query metrics --name http.server.duration --service api --last 5m

devrig query logs                       # Recent structured logs (JSON)
devrig query logs --service api --level error
devrig query logs --search "connection refused"
devrig query logs --trace-id <id>       # Logs correlated to a trace

devrig query status                     # Full system status (services, docker, cluster)

# Cross-telemetry lookups (jump-to from CLI)
devrig query related <trace-id>         # All telemetry related to a trace: logs, metrics, spans
devrig query logs --around <trace-id>   # Logs from same service during trace's time window
devrig query traces --around <timestamp> --service api --window 10s
                                        # Traces near a point in time (for metric correlation)
devrig query logs --from 2026-02-21T10:30:00Z --to 2026-02-21T10:30:10Z --service api
                                        # Logs in an exact time window
```

### Output format

All `devrig query` commands output newline-delimited JSON by default:

```json
{"traceId":"abc123","service":"api","operation":"POST /users","duration_ms":42,"status":"ok","spans":12,"timestamp":"2026-02-21T10:30:00Z"}
{"traceId":"def456","service":"api","operation":"GET /users/1","duration_ms":180,"status":"error","spans":8,"timestamp":"2026-02-21T10:30:01Z"}
```

Flags: `--format json` (default), `--format json-pretty`, `--format table` (human-readable).

### Claude Code skill

devrig ships a built-in Claude Code skill that gives Claude full observability into your running dev environment. The skill is a directory (`skill/claude-code/`) in the devrig repo, ready to be installed into any project or globally.

#### Skill structure

```
skill/claude-code/
  SKILL.md                 # Skill instructions for Claude
  devrig.sh                # Helper script wrapping devrig query commands
```

#### SKILL.md (shipped with devrig)

```markdown
# devrig — Development Environment Observability

You have access to a running devrig development environment. Use the `devrig` CLI
to inspect services, infrastructure, traces, metrics, and logs.

## Available commands

### Status
- `devrig ps` — Show all services and docker containers with health status
- `devrig query status` — Full system status as JSON

### Traces
- `devrig query traces --service <name>` — Recent traces for a service
- `devrig query traces --status error` — All errored traces
- `devrig query traces --min-duration <duration>` — Slow traces
- `devrig query trace <trace-id>` — Full trace detail with spans
- `devrig query related <trace-id>` — All correlated logs, metrics, spans

### Metrics
- `devrig query metrics` — List all known metric names
- `devrig query metrics --name <metric> --service <name> --last <duration>`

### Logs
- `devrig query logs --service <name> --level error` — Error logs
- `devrig query logs --search "<text>"` — Search log bodies
- `devrig query logs --trace-id <id>` — Logs correlated to a trace
- `devrig query logs --around <trace-id>` — Logs in the trace's time window

### Cross-telemetry
- `devrig query traces --around <timestamp> --service <name> --window 10s`
- `devrig query logs --from <iso8601> --to <iso8601> --service <name>`

### Service Discovery
- `devrig env <service>` — All resolved env vars for a service (ports, URLs, hosts)
- `devrig env <service> DEVRIG_POSTGRES_URL` — A specific resolved var

### Actions
- `devrig restart <service>` — Restart a service
- `devrig exec <docker>` — Shell into a docker container
- `devrig k get pods` — List pods in the k3d cluster
- `devrig k logs deploy/<name>` — Tail logs from a cluster deployment

## Output format

All `devrig query` commands return newline-delimited JSON by default.
Use `--format json-pretty` for readable output during investigation.

## Workflow guidance

When debugging performance issues:
1. Start with `devrig query traces --service <name> --min-duration 500ms`
2. Pick the slowest trace, examine spans with `devrig query trace <id>`
3. Correlate with `devrig query related <id>` to see logs and metrics
4. Check for errors with `devrig query logs --trace-id <id> --level error`

When investigating errors:
1. Start with `devrig query traces --status error --service <name>`
2. Or `devrig query logs --service <name> --level error`
3. Cross-reference with `devrig query related <trace-id>`

When checking system health:
1. `devrig query status` for an overview
2. `devrig query metrics --name http.server.duration --service <name> --last 5m`
3. `devrig query logs --service <name> --level warn --level error`
```

#### Installation

```bash
# Install to current project (adds to .claude/skills/)
devrig skill install

# Install globally (adds to ~/.claude/skills/)
devrig skill install --global
```

`devrig skill install` copies the skill directory into the target location. The project-local install is preferred — it means anyone cloning the repo gets the skill automatically when they use Claude Code in that project.

After installation, Claude Code sees devrig as a skill and can autonomously query traces, metrics, and logs to debug issues, investigate performance, and correlate telemetry.

#### What the skill enables

With the skill installed, a developer can say things like:

- *"Why is the API slow?"* — Claude queries slow traces, identifies the bottleneck span, correlates with logs, and suggests a fix.
- *"What's erroring in the worker service?"* — Claude queries error traces and logs, cross-references with metrics, and pinpoints the issue.
- *"Show me what happened at 10:30am"* — Claude uses time-window queries across all three telemetry types to reconstruct what occurred.
- *"Is postgres healthy?"* — Claude checks docker status, connection metrics, and recent error logs.

The structured JSON output means Claude can ingest, filter, and reason about telemetry data without parsing human-readable output.

### HTTP API (alternative)

The same data is available via REST for tools that prefer HTTP:

```
GET /api/traces?service=api&status=error
GET /api/traces/:traceId
GET /api/traces/:traceId/related        # All related telemetry (logs, metrics) for a trace
GET /api/metrics?name=http.server.duration&last=5m
GET /api/logs?service=api&level=error
GET /api/logs?around=:traceId           # Logs correlated to a trace (by ID or time window)
GET /api/logs?service=api&from=...&to=... # Logs in exact time window
GET /api/traces?service=api&around=:timestamp&window=10s  # Traces near a timestamp
GET /api/status
GET /api/env                                      # All resolved env vars for all services
GET /api/env/:service                             # All resolved env vars for a specific service
```

Served from the same Axum server as the dashboard.

---

## Cluster addons — Easy deployment of dev tools to k3d

When using a k3d cluster, you often want supporting tools deployed alongside your services — things like Headlamp for a Kubernetes dashboard, cert-manager, ingress controllers, etc. devrig makes this a one-liner in config.

### Configuration

```toml
[cluster.addons.headlamp]
type = "helm"                          # Install via Helm chart
chart = "headlamp/headlamp"
repo = "https://headlamp-k8s.github.io/headlamp/charts"
namespace = "headlamp"
values = { service.type = "NodePort" }
port_forward = { 8080 = "svc/headlamp:80" }  # Auto port-forward to localhost

[cluster.addons.traefik]
type = "helm"                          # Install via Helm chart
chart = "traefik/traefik"
repo = "https://traefik.github.io/charts"
namespace = "traefik"
values = { ports.web.nodePort = 32080 }
port_forward = { 9000 = "svc/traefik:9000" }  # Traefik dashboard

[cluster.addons.cert-manager]
type = "helm"
chart = "jetstack/cert-manager"
repo = "https://charts.jetstack.io"
namespace = "cert-manager"
values = { installCRDs = true }

[cluster.addons.my-tool]
type = "manifest"
path = "./k8s/addons/my-tool.yaml"     # Local manifest file
```

### Addon types

| Type | Source | Use case |
|---|---|---|
| `helm` | Helm chart from a repo | Most third-party tools (Traefik, Headlamp, cert-manager, etc.) |
| `manifest` | URL or local file | Simple deployments, CRDs, one-off resources |
| `kustomize` | Kustomize directory | When you need overlays or patches |

### Lifecycle

Addons are installed during Phase 2 (cluster setup), after the cluster is created but before your services are deployed. They follow the same `depends_on` system — a service can depend on an addon being ready.

`devrig delete` tears down addons with the cluster. `devrig stop` leaves them running (the cluster stays up, just idle).

### Port forwarding

The `port_forward` field automatically sets up `kubectl port-forward` for the addon, so tools like Headlamp are accessible at `http://localhost:<port>` without manual setup. These show up in the startup summary alongside your services.

---

## Service auto-detection (future, opt-in)

For common project types, devrig could infer defaults:

```toml
[services.api]
path = "./services/api"
# If path contains Cargo.toml → command defaults to "cargo watch -x run"
# If path contains package.json with "dev" script → command defaults to "npm run dev"
# Port still required (no magic)
```

This is opt-in via `detect = true` on the service. Explicit config always wins.

---

## Milestones

### v0.1 — Local process orchestration
- Parse `devrig.toml` with `[project]`, `[services.*]`, `[env]`
- `-f` flag for alternate config files
- Project identity (name + path hash) for multi-instance isolation
- Directory-aware config discovery (walk up directory tree)
- `devrig start` / `stop` / `delete` for local processes
- Startup summary with service URLs, project slug, and status
- Port collision detection with clear error messages identifying the conflicting project
- Dependency ordering with `depends_on`
- Unified colored log output
- `devrig ps` status display, `devrig ps --all` for cross-project discovery
- `devrig init` scaffolding
- `devrig doctor` dependency checker
- **Tests:** Unit tests for config parsing + dependency resolution + project identity hashing. Integration tests for start/stop/delete lifecycle, `-f` flag, process crash recovery, port verification, cleanup assertions. Multi-instance tests: two projects running simultaneously with no cross-talk, port collision detection, `ps --all` discovery, label-scoped cleanup, directory tree config discovery.
- **Docs:** `README.md` (quickstart, demo placeholder, minimal example). Initial ADRs (001–008 for all existing decisions, including multi-instance isolation). `docs/architecture/overview.md`, `docs/architecture/config-model.md`, `docs/architecture/dependency-graph.md`, `docs/architecture/multi-instance.md`. `docs/guides/getting-started.md`, `docs/guides/configuration.md`, `docs/guides/contributing.md`.

### v0.2 — Docker containers
- `[docker.*]` blocks with Docker container lifecycle
- Image pulling, volume management
- Ready check system (all built-in strategies)
- Init script execution and tracking
- Service discovery: `DEVRIG_<NAME>_HOST`, `_PORT`, `_URL` for all services and docker containers
- URL generation (postgres://, redis://, http://) from config + credentials
- `devrig env <service>` command for inspecting resolved vars
- Auto port selection (`port = "auto"`) with state persistence
- Template expression resolution (`{{ docker.postgres.port }}`, `{{ services.api.port }}`)
- `devrig exec` and `devrig reset`
- Docker network creation and management
- `[compose]` interop — delegate docker containers to existing `docker-compose.yml`
- Compose + native docker coexistence on shared network
- **Tests:** Integration tests for Postgres/Redis lifecycle (start, ready check, connect, init SQL, stop, delete). Service discovery tests: verify `DEVRIG_*` vars injected into processes, URL generation correctness, `devrig env` output, auto port persistence across stop/start. Compose interop tests. Volume cleanup assertions. Network isolation verification. Leaked resource checks.
- **Docs:** `docs/architecture/service-discovery.md`. `docs/guides/compose-migration.md`. Update config reference with `[docker.*]`, `[compose]`, service discovery vars, and template expressions.

### v0.3 — k3d cluster support
- `[cluster]` configuration
- k3d cluster create/delete lifecycle
- Local registry support
- `[cluster.deploy.*]` with build + manifest apply
- File watching for cluster-deployed services
- Network bridging between Docker network and cluster
- **Tests:** Integration tests for full cluster lifecycle (create, deploy, verify pod running, `devrig k get pods` works, delete cluster, verify no k3d resources remain, verify `.devrig/kubeconfig` removed, verify `~/.kube/config` untouched). Registry push/pull validation. Network bridge connectivity between docker containers and cluster pods.
- **Docs:** `docs/guides/cluster-setup.md`, `docs/architecture/kubeconfig-isolation.md`. Update config reference with `[cluster]` and `[cluster.deploy.*]`.

### v0.4 — Developer experience polish
- `devrig validate` with helpful error messages
- Config file watching (auto-restart on `devrig.toml` changes)
- Crash recovery with exponential backoff
- `devrig logs` filtering, search, and export
- Shell completions (bash, zsh, fish)
- Colored, structured terminal UI (service status dashboard)

### v0.5 — Observability + Dashboard
- Axum backend serving SolidJS + Solid UI frontend
- In-process OTLP receiver (gRPC + HTTP)
- In-memory ring buffer storage for traces, metrics, logs
- Auto-injection of `OTEL_EXPORTER_OTLP_ENDPOINT` into services
- Dashboard: overview, trace waterfall, metric charts, structured log viewer
- Real-time updates via WebSocket
- `devrig query` CLI for machine-readable observability data (JSON)
- REST API mirroring CLI query capabilities
- Service management from the UI (start/stop/restart)
- **Tests:** Integration tests for OTLP ingest (send spans/metrics/logs, verify ring buffer contents via query CLI). Agent-browser E2E tests for all dashboard views — overview status, trace waterfall rendering, metric chart discovery, log filtering, trace correlation navigation, Cmd+K palette, dark/light toggle, real-time WebSocket push.
- **Docs:** `docs/architecture/otel-storage.md`. `docs/api/rest-api.md`, `docs/api/query-cli.md`. Update config reference with `[dashboard]` and `[dashboard.otel]`.

### v0.6 — Claude Code skill + Cluster addons
- Claude Code skill: `skill/claude-code/SKILL.md` with full query workflow guidance
- `devrig skill install` (project-local to `.claude/skills/`) and `devrig skill install --global` (`~/.claude/skills/`)
- `[cluster.addons.*]` with Helm, manifest, and Kustomize support (prefer Traefik over nginx-ingress)
- Automatic port-forwarding for addon UIs
- Config editor with validation in dashboard
- **Tests:** Integration tests for `devrig skill install` (verify files copied to correct location, project-local and global). Integration tests for addon install/teardown (Traefik Helm chart deploys, port-forward works, cleanup on delete). Agent-browser E2E for config editor validation. Skill validation: run Claude Code with the installed skill against a live devrig instance, verify it can query traces/logs/metrics and produce meaningful analysis.
- **Docs:** `docs/guides/claude-code-skill.md` — how to install and use the skill, example prompts, what Claude can do with it. Update config reference with `[cluster.addons.*]`.

---

## Testing Strategy

Every milestone must ship with tests that prove the feature actually works end-to-end — not just unit tests asserting struct shapes. The testing philosophy: **if it can spin up, it must be tested spinning up. If it can fail, the failure path must be tested too.**

### Test layers

**Unit tests** — Standard Rust `#[test]` for pure logic: config parsing, dependency graph resolution, template interpolation, ready check polling logic, ring buffer eviction, TOML validation. These run fast, no Docker needed.

**Integration tests** — Require Docker. These are the critical layer. Each test creates real containers, real processes, real networks — then tears them all down. Run with `cargo test --features integration`.

```
tests/
  integration/
    start_stop.rs          # Start services, verify running, stop, verify stopped
    delete_cleanup.rs      # Delete removes containers, volumes, networks, state
    infra_lifecycle.rs     # Postgres/Redis containers start, pass ready checks, accept connections
    compose_interop.rs     # docker-compose.yml services start alongside native docker
    depends_on.rs          # Services start in correct order, wait for ready checks
    env_injection.rs       # DEVRIG_* vars and explicit env are present in service processes
    service_discovery.rs   # DEVRIG_<NAME>_URL, _HOST, _PORT injected for all services/docker
    env_command.rs         # devrig env <service> returns correct resolved vars
    url_generation.rs      # Postgres/Redis/HTTP URL generation from config + credentials
    init_scripts.rs        # SQL init runs once, tracked in state, skipped on re-start
    crash_recovery.rs      # Service crashes → devrig restarts with backoff
    config_file_flag.rs    # -f flag loads alternate config
    multi_instance.rs      # Two projects run simultaneously, resources isolated, no cross-talk
    port_collision.rs      # Second project with same port fails with clear error, names the conflict
    ps_all.rs              # devrig ps --all discovers all running instances via Docker labels
    label_cleanup.rs       # Delete only removes resources with matching project label
    dir_discovery.rs       # devrig finds config by walking up directory tree
    cluster_lifecycle.rs   # k3d cluster creates, deploys, tears down (v0.3+)
    dashboard_startup.rs   # Dashboard serves on configured port, WebSocket connects (v0.5+)
    otel_ingest.rs         # Send OTLP data → verify it appears in ring buffers (v0.5+)
    query_cli.rs           # devrig query traces/metrics/logs returns valid JSON (v0.5+)
```

**Each integration test follows a strict pattern:**

```rust
#[tokio::test]
async fn test_start_stop_lifecycle() {
    // 1. Setup: write a devrig.toml to a temp dir
    let dir = TempDir::new().unwrap();
    write_config(&dir, r#"
        [project]
        name = "test-lifecycle"
        [services.echo]
        command = "python3 -m http.server 8111"
        port = 8111
    "#);

    // 2. Act: run devrig start
    let rig = DevRig::from_config(dir.path()).await.unwrap();
    rig.start().await.unwrap();

    // 3. Assert: service is actually reachable
    wait_for_http("http://localhost:8111", Duration::from_secs(10)).await.unwrap();

    // 4. Act: stop
    rig.stop().await.unwrap();

    // 5. Assert: service is actually gone
    assert!(TcpStream::connect("127.0.0.1:8111").await.is_err());

    // 6. Cleanup: always runs, even on panic
    // (TempDir drop + rig drop handle cleanup)
}
```

**Key invariants every integration test must verify:**

- **Start actually starts.** HTTP/TCP checks confirm the service is accepting connections, not just that devrig thinks it started.
- **Stop actually stops.** Ports are released. Processes are gone. `docker ps` shows no orphaned containers.
- **Delete actually cleans up.** No volumes, no networks, no containers, no k3d cluster, no `.devrig/kubeconfig`, no `.devrig/state.json`. A second `devrig start` from scratch must work identically. `~/.kube/config` is never touched.
- **No leaked resources.** Every test gets a unique project slug (via unique temp directory paths). All Docker resources are labeled with `devrig.project={slug}`. After teardown, assert zero resources with that label remain. The project-scoped labeling that enables multi-instance isolation also makes test cleanup bulletproof.

### Agent-browser E2E validation (dashboard)

For the dashboard (v0.5+), use the Claude Code agent-browser skill to validate the UI end-to-end. The agent navigates the actual dashboard in a real browser, verifying what a human would see.

```
e2e/
  dashboard/
    overview.test.ts       # Services and docker containers appear with correct status indicators
    traces.test.ts         # Send test spans → verify waterfall renders, filters work
    metrics.test.ts        # Send test metrics → verify charts render, auto-discover works
    logs.test.ts           # Send test logs → verify log lines appear, severity filter works
    trace_correlation.ts   # Click log with trace context → navigates to trace view
    metric_to_traces.ts    # Click metric chart point → shows traces in time window
    trace_to_logs.ts       # Trace span detail → "View logs" shows correlated logs
    trace_to_metrics.ts    # Trace span detail → "View metrics" shows metrics at span time
    keyboard_jumps.ts      # t/l/m keys jump between related telemetry views
    deep_links.ts          # URL-based deep links load correct filtered views
    cmd_k.test.ts          # Cmd+K opens palette, can jump between views
    dark_light.test.ts     # Theme toggle works, persists across refresh
    realtime.test.ts       # New data appears without refresh (WebSocket push)
    overview_actions.ts    # Restart/stop buttons work, status updates live
```

**How these run:**

1. A test fixture starts `devrig start` with a known config (services + docker + dashboard)
2. A test harness sends synthetic OTLP data (traces, metrics, logs) to the collector
3. The agent-browser skill opens `http://localhost:4000` and validates the UI
4. Assertions check that elements are present, filters produce correct results, navigation works
5. Teardown runs `devrig delete` and verifies clean state

This replaces flaky unit-testing of SolidJS components with real browser validation of the actual running system.

### CI pipeline

```
┌─────────────────────────────────────────────────┐
│ CI                                              │
├─────────────────────────────────────────────────┤
│ 1. cargo fmt --check                            │
│ 2. cargo clippy -- -D warnings                  │
│ 3. cargo test                    (unit tests)   │
│ 4. cargo test --features integration             │
│    ├─ Requires: Docker                          │
│    ├─ Requires: k3d (for cluster tests)         │
│    └─ Each test gets isolated resources          │
│ 5. E2E dashboard tests                          │
│    ├─ Requires: Docker + browser                │
│    └─ Runs devrig start → browser tests → delete│
│ 6. Leak check: assert no devrig-test-* resources│
└─────────────────────────────────────────────────┘
```

### Testing rules (enforced during development)

1. **No milestone ships without integration tests for every new feature.** If the feature touches Docker, k3d, or process management, it needs a real integration test.
2. **Every `start` test must have a matching cleanup assertion.** No test is complete until it proves resources are gone.
3. **Tests must be parallelizable.** Unique project names, unique ports, unique Docker labels per test. No shared state.
4. **Flaky tests are bugs.** If a test flakes, fix the timing/polling, don't skip it.
5. **Dashboard tests use agent-browser, not component unit tests.** The browser is the source of truth for UI correctness.

---

## Decisions

1. **Config format:** TOML only. Stay opinionated. One format, no ambiguity.

2. **Secrets:** Not a concern for v1. This is local dev — hardcoded passwords in `devrig.toml` are fine. Secrets management can come later if devrig expands to staging/CI use cases.

3. **Profiles/environments:** No profiles system. Instead, use separate TOML files and the `-f` flag: `devrig start -f devrig.minimal.toml`. All-or-nothing per file. Simple, composable, no new concepts.

4. **Plugin system:** No plugin system for now. Keep `[docker.*]` as raw image + config. The built-in `ready_check` strategies and `init` blocks cover the common cases (Postgres, Redis, etc.) without needing abstraction. If patterns emerge, revisit later.

5. **Compose interop:** Yes. devrig can import a `docker-compose.yml` for the docker layer via `[compose]`. See the Compose Interop section below.

6. **Multi-repo:** Relative paths only. No absolute paths. If devrig runs on different machines (which it will), absolute paths break. For multi-repo, the `devrig.toml` lives in a parent directory or a dedicated orchestration repo, and all paths point to sibling repos with `../`.

---

## Compose Interop

devrig can delegate infrastructure management to an existing `docker-compose.yml` instead of (or alongside) native `[docker.*]` blocks. This is useful for teams that already have a working Compose setup and don't want to rewrite it.

### Configuration

```toml
[compose]
file = "./docker-compose.yml"          # Path to compose file (relative)
services = ["postgres", "redis"]       # Which compose services to manage (optional — all by default)
env_file = ".env"                      # Compose env file (optional)
```

### How it works

When `[compose]` is present, devrig runs `docker compose up -d` for the specified services during Phase 1 (infrastructure). It uses the Docker Compose API (via `bollard` or shelling out to `docker compose`) to manage lifecycle.

devrig still handles ready checks, dependency ordering, and environment variable injection for compose-managed services. You can reference compose services in `depends_on` just like native `[docker.*]` blocks:

```toml
[compose]
file = "./docker-compose.yml"
services = ["postgres", "redis"]

[compose.ready_checks]
postgres = { type = "pg_isready" }
redis = { type = "cmd", command = "redis-cli ping" }

[services.api]
depends_on = ["postgres"]             # Works — postgres is from compose
```

### Coexistence with native docker

`[compose]` and `[docker.*]` can coexist. Use compose for what you already have, native docker for anything new. devrig puts everything on the same Docker network regardless of source.

```toml
# Existing compose services
[compose]
file = "./docker-compose.yml"
services = ["postgres"]

# New docker containers managed natively
[docker.redis]
image = "redis:7-alpine"
port = 6379
```

### Migration path

Teams can start with `[compose]` to get devrig's orchestration benefits immediately, then gradually migrate services to native `[docker.*]` blocks for tighter integration (auto-generated env vars, init scripts, etc.).
