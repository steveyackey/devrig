# devrig Configuration Reference

Config file: `devrig.toml`. Located by walking up from cwd; override with `-f <path>`.

## Contents
- [`[project]`](#project)
- [`[env]`](#env)
- [`[services.*]`](#services) — restart config
- [`[docker.*]`](#docker) — ready check types
- [`[dashboard]` / `[dashboard.otel]`](#dashboard)
- [`[compose]`](#compose)
- [`[cluster]`](#cluster) — registries, deploy, addons
- [`[links]`](#links)
- [`[network]`](#network)
- [Environment variable expansion](#environment-variable-expansion)
- [Template expressions](#template-expressions)
- [Auto-injected `DEVRIG_*` variables](#auto-injected-devrig_-variables)
- [OTEL auto-injection](#otel-auto-injection)

---

## `[project]` (required)

| Field      | Type   | Required | Description                        |
|------------|--------|----------|------------------------------------|
| `name`     | string | Yes      | Project name for display and slug  |
| `env_file` | string | No       | Path to project-level `.env` file  |

---

## `[env]`

Global env vars passed to every service. Supports `{{ }}` template expressions. Per-service env overrides these.

```toml
[env]
RUST_LOG = "debug"
NODE_ENV = "development"
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
```

---

## `[services.*]`

| Field        | Type               | Required | Default      | Description                                  |
|--------------|--------------------|----------|--------------|----------------------------------------------|
| `command`    | string             | Yes      | --           | Shell command (via `sh -c`)                  |
| `path`       | string             | No       | config dir   | Working directory relative to config file    |
| `port`       | int or `"auto"`    | No       | (none)       | Port the service listens on                  |
| `protocol`   | string             | No       | `"http"`     | Port protocol: `"http"`, `"https"`, `"tcp"`, `"udp"`. Controls dashboard link scheme. |
| `env`        | map                | No       | `{}`         | Service-specific env vars                    |
| `env_file`   | string             | No       | (none)       | Per-service `.env` file path                 |
| `depends_on` | list               | No       | `[]`         | Services/docker/compose to start before this |

**Port values:** `3000` (fixed, verified available), `"auto"` (ephemeral, sticky across restarts), omitted (no management). When set, `PORT` env var is injected. **Prefer `"auto"` unless the service requires a specific port** (e.g. well-known ports for external clients, callback URLs). Auto ports avoid conflicts and are stable across restarts.

### `[services.<name>.restart]`

| Field                  | Type    | Default      | Description                    |
|------------------------|---------|--------------|--------------------------------|
| `policy`               | string  | `on-failure` | `always`, `on-failure`, `never`|
| `max_restarts`         | int     | `10`         | Max restarts during runtime    |
| `startup_max_restarts` | int     | `3`          | Max restarts in startup phase  |
| `startup_grace_ms`     | int     | `2000`       | Startup phase duration (ms)    |
| `initial_delay_ms`     | int     | `500`        | Initial backoff delay (ms)     |
| `max_delay_ms`         | int     | `30000`      | Max backoff delay (ms)         |

---

## `[docker.*]`

| Field           | Type               | Required | Default | Description                              |
|-----------------|--------------------|----------|---------|------------------------------------------|
| `image`         | string             | Yes      | --      | Docker image                             |
| `port`          | int or `"auto"`    | No       | (none)  | Host port mapping                        |
| `container_port`| int                | No       | same as `port` | Internal port inside container (when host ≠ container port) |
| `protocol`      | string             | No       | `"http"` | Port protocol: `"http"`, `"https"`, `"tcp"`, `"udp"`. Controls dashboard link scheme. |
| `ports`         | map                | No       | `{}`    | Named port mappings (multi-port)         |
| `env`           | map                | No       | `{}`    | Container env vars                       |
| `volumes`       | list               | No       | `[]`    | Volume mounts: named (`"vol:/path"`) or bind (`"/host:/path"`, `"./rel:/path"`) |
| `command`       | string or list     | No       | (none)  | Override image CMD                       |
| `entrypoint`    | string or list     | No       | (none)  | Override image ENTRYPOINT                |
| `ready_check`   | table              | No       | (none)  | Health check config                      |
| `init`          | list               | No       | `[]`    | SQL/commands after first ready           |
| `depends_on`    | list               | No       | `[]`    | Other docker/compose dependencies        |
| `registry_auth` | table              | No       | (none)  | Private registry credentials (`username`, `password`) |

### Ready check types

| Type         | Runs      | Description                                 |
|--------------|-----------|---------------------------------------------|
| `pg_isready` | container | `pg_isready -h localhost -q -t 2` (30s)     |
| `cmd`        | container | Custom command; optional `expect` string    |
| `http`       | host      | GET request, checks for 2xx (30s)           |
| `tcp`        | host      | TCP connection to host port (30s)           |
| `log`        | container | Stream logs, match pattern (60s)            |

All ready check types support an optional `timeout` field (seconds) to override the default.

```toml
ready_check = { type = "pg_isready" }
ready_check = { type = "cmd", command = "redis-cli ping", expect = "PONG" }
ready_check = { type = "http", url = "http://localhost:9000/health" }
ready_check = { type = "http", url = "http://localhost:8080/health", timeout = 90 }
ready_check = { type = "tcp" }
[docker.es.ready_check]
type = "log"
match = "started"
timeout = 120
```

---

## `[dashboard]`

| Field     | Type            | Default | Description                         |
|-----------|-----------------|---------|-------------------------------------|
| `port`    | int or `"auto"` | `4000`  | Dashboard web UI and API port       |
| `enabled` | bool            | `true`  | Whether to start the dashboard      |

### `[dashboard.otel]`

| Field           | Type            | Default   | Description                        |
|-----------------|-----------------|-----------|-------------------------------------|
| `grpc_port`     | int or `"auto"` | `4317`    | OTLP gRPC receiver port            |
| `http_port`     | int or `"auto"` | `4318`    | OTLP HTTP receiver port            |
| `trace_buffer`  | int     | `10000`   | Max spans in memory                |
| `metric_buffer` | int     | `50000`   | Max metric data points             |
| `log_buffer`    | int     | `100000`  | Max log records                    |
| `retention`     | string  | `"1h"`    | Retention duration (e.g. `"2h30m"`)|

---

## `[compose]`

| Field          | Type    | Required | Default | Description                                       |
|----------------|---------|----------|---------|---------------------------------------------------|
| `file`         | string  | Yes      | --      | Path to docker-compose.yml                        |
| `services`     | list    | No       | `[]`    | Services to start (auto-discovered if empty)      |
| `env_file`     | string  | No       | (none)  | Env file for compose                              |
| `ready_checks` | map     | No       | `{}`    | Ready checks for compose services                 |

---

## `[cluster]`

| Field      | Type    | Default         | Description                    |
|------------|---------|-----------------|--------------------------------|
| `name`     | string  | `devrig-{slug}` | k3d cluster name               |
| `agents`   | int     | `1`             | Number of agent nodes          |
| `ports`    | list    | `[]`            | Host-to-cluster port mappings  |
| `registry` | bool    | `true`          | Create local container registry|
| `k3s_args` | list    | `[]`            | Extra args passed to k3s via `--k3s-arg` |

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

| Field           | Type    | Required | Default      | Description                         |
|-----------------|---------|----------|--------------|-------------------------------------|
| `context`       | string  | Yes      | --           | Docker build context dir            |
| `dockerfile`    | string  | No       | `Dockerfile` | Dockerfile path relative to context |
| `manifests`     | list    | Yes      | --           | K8s manifest files to apply         |
| `watch`         | bool    | No       | `false`      | Auto-rebuild on file changes        |
| `depends_on`    | list    | No       | `[]`         | Docker/deploy dependencies          |
| `build_secrets` | map     | No       | `{}`         | BuildKit secrets: `{ id = "~/path" }` → `--secret id=<key>,src=<path>` |
| `build_args`    | map     | No       | `{}`         | Docker build args: `{ KEY = "value" }` → `--build-arg KEY=value`. Supports `{{ cluster.image.<name>.tag }}` interpolation. |

### `[cluster.addons.*]`

Types: `helm`, `manifest`, `kustomize`. All support `namespace`, `port_forward`, and `depends_on`.

- **Helm**: `chart` (required), `repo` (optional — omit for local charts), `version`, `values` (supports `{{ }}` templates), `values_files`, `wait` (default: `true`), `timeout` (default: `"5m"`), `skip_crds` (default: `false` — pass `--skip-crds` to helm)
- **Manifest**: `path` (required)
- **Kustomize**: `path` (required)

Addons install in dependency order (topological sort, alphabetical tie-break).

```toml
# Remote chart
[cluster.addons.cert-manager]
type = "helm"
chart = "cert-manager/cert-manager"
repo = "https://charts.jetstack.io"
namespace = "cert-manager"

# Local chart with dependencies and image tag template
[cluster.addons.myapp]
type = "helm"
chart = "./charts/myapp"
namespace = "myapp"
depends_on = ["cert-manager"]
wait = false
timeout = "10m"
[cluster.addons.myapp.values]
"image.tag" = "{{ cluster.image.myapp.tag }}"
```

---

## `[links]`

Named URLs for services devrig doesn't manage (e.g., deployed by Flux). Shown in dashboard with blue indicator and clickable link.

| Field    | Type   | Required | Description                |
|----------|--------|----------|----------------------------|
| `<name>` | string | --       | Display name → URL mapping |

```toml
[links]
headlamp = "http://localhost:8080"
grafana = "http://localhost:3000"
```

---

## `[network]`

| Field  | Type   | Default             | Description           |
|--------|--------|---------------------|-----------------------|
| `name` | string | `devrig-{slug}-net` | Custom Docker network |

---

## Environment Variable Expansion

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

---

## Template Expressions

`[env]`, `[services.*.env]`, and addon `values` support `{{ dotted.path }}` templates:

| Variable                             | Example value                 | Context                    |
|--------------------------------------|-------------------------------|----------------------------|
| `project.name`                       | `myapp`                       | All                        |
| `services.<name>.port`               | `3000`                        | All                        |
| `docker.<name>.port`                 | `5432`                        | All                        |
| `docker.<name>.ports.<portname>`     | `1025`                        | All                        |
| `docker.<name>.port_<portname>`      | `1025`                        | All (alias for `ports.*`)  |
| `compose.<name>.port`                | `6379`                        | All                        |
| `cluster.name`                       | `myapp-dev`                   | All (when cluster defined) |
| `cluster.kubeconfig`                 | `.devrig/myapp-abc123/kubeconfig` | Service env (when cluster defined) |
| `cluster.registry`                   | `k3d-devrig-abc123-reg:5000`  | Addon values (when registry enabled) |
| `cluster.image.<name>.tag`           | `1234567890`                  | Addon values + service env |
| `dashboard.port`                     | `4000`                        | All                        |
| `dashboard.otel.grpc_port`           | `4317`                        | All                        |
| `dashboard.otel.http_port`           | `4318`                        | All                        |

Unresolved variables produce an error with a "did you mean?" suggestion if a close match exists.

```toml
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/mydb"
```

---

## Auto-injected `DEVRIG_*` Variables

Every service receives discovery vars for all other services and docker containers:

| Variable                          | Example                              |
|-----------------------------------|--------------------------------------|
| `DEVRIG_<NAME>_HOST`              | `localhost`                          |
| `DEVRIG_<NAME>_PORT`              | `5432`                               |
| `DEVRIG_<NAME>_URL`               | `postgres://user:pass@localhost:5432`|
| `DEVRIG_<NAME>_PORT_<PORTNAME>`   | `1025` (for named ports)             |

---

## OTEL Auto-injection

When dashboard is enabled, every service gets:

| Variable                          | Description                                  |
|-----------------------------------|----------------------------------------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT`    | OTLP gRPC endpoint (`http://localhost:4317`) |
| `OTEL_SERVICE_NAME`               | Service name from config                     |
| `DEVRIG_DASHBOARD_URL`            | Dashboard URL (`http://localhost:4000`)       |
