# Configuration Guide

This is the full reference for `devrig.toml`.

## File location

devrig looks for `devrig.toml` by walking up the directory tree from the
current working directory. The first `devrig.toml` found is used.

To use a different file, pass `-f`:

```bash
devrig start -f devrig.staging.toml
```

## `[project]` section

The project section is **required** and defines project-level metadata.

```toml
[project]
name = "myapp"
env_file = ".env"     # optional: load shared secrets
```

| Field      | Type   | Required | Default | Description                                        |
|------------|--------|----------|---------|----------------------------------------------------|
| `name`     | string | Yes      | --      | Project name. Used in the slug and display output.  |
| `env_file` | string | No       | (none)  | Path to a `.env` file with shared secrets.          |

The project name combined with a hash of the config file path forms the
project slug (e.g. `myapp-a1b2c3d4`), which is used for state isolation.

## `[services.*]` section

Each `[services.<name>]` block defines a local process service that devrig
manages. The `<name>` is an arbitrary identifier used in logs, status output,
and `depends_on` references.

### Minimal service

```toml
[services.api]
command = "cargo watch -x run"
```

### Full service

```toml
[services.api]
path = "./services/api"
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres", "redis"]

[services.api.env]
API_KEY = "dev-secret"
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
```

### Fields

| Field        | Type               | Required | Default | Description                                              |
|--------------|--------------------|----------|---------|----------------------------------------------------------|
| `command`    | string             | Yes      | --      | Shell command to run (executed via `sh -c`).              |
| `path`       | string             | No       | (none)  | Working directory, relative to the config file.           |
| `port`       | integer or `"auto"`| No       | (none)  | Port the service listens on.                              |
| `env`        | map of strings     | No       | `{}`    | Environment variables for this service.                   |
| `env_file`   | string             | No       | (none)  | Path to a `.env` file for this service.                   |
| `depends_on` | list of strings    | No       | `[]`    | Services, docker, or compose services to start before this.|

### Port values

The `port` field accepts three forms:

```toml
port = 3000       # Fixed port. devrig verifies it is available at startup.
port = "auto"     # devrig assigns a free ephemeral port.
# (omitted)       # No port management. The service manages its own port.
```

When a port is specified (fixed or auto), devrig sets the `PORT` environment
variable so the service can discover its assigned port. Auto-assigned ports
are sticky across restarts -- devrig reuses the same port if it is still
available.

### Command execution

The `command` string is passed to `sh -c`, so shell features (pipes,
redirects, `&&` chains) work:

```toml
command = "cargo watch -x run"
command = "npm run dev -- --port $PORT"
```

Each service runs in its own process group. On shutdown, SIGTERM is sent to
the entire group.

### Working directory

The `path` field is resolved relative to the config file location, not the
current working directory. If omitted, the service runs in the directory
containing `devrig.toml`.

### Restart configuration

Each service can have a `[services.<name>.restart]` section to control
restart behavior on crashes:

```toml
[services.api.restart]
policy = "on-failure"        # "always", "on-failure", or "never"
max_restarts = 10            # Max restarts in runtime phase
startup_max_restarts = 3     # Max restarts during startup grace period
startup_grace_ms = 2000      # Duration (ms) considered "startup phase"
initial_delay_ms = 500       # Initial backoff delay before first restart
max_delay_ms = 30000         # Maximum backoff delay
```

| Field                  | Type    | Default      | Description                               |
|------------------------|---------|--------------|-------------------------------------------|
| `policy`               | string  | `on-failure` | When to restart: `always`, `on-failure`, `never` |
| `max_restarts`         | integer | `10`         | Max restarts during runtime                |
| `startup_max_restarts` | integer | `3`          | Max restarts during startup grace period   |
| `startup_grace_ms`     | integer | `2000`       | Startup phase duration in milliseconds     |
| `initial_delay_ms`     | integer | `500`        | Initial backoff delay in milliseconds      |
| `max_delay_ms`         | integer | `30000`      | Maximum backoff delay in milliseconds      |

Restart policies:
- **`on-failure`** (default): Restart only if the process exits with a non-zero code.
- **`always`**: Restart regardless of exit code, including clean exits.
- **`never`**: Never restart. The service stays down after any exit.

If omitted, the service uses sensible defaults (on-failure with exponential
backoff).

### Dependencies

The `depends_on` list controls startup order. Dependencies can reference
service names, docker names, or compose service names:

```toml
[services.api]
command = "cargo run"
depends_on = ["postgres", "redis"]  # postgres is [docker.postgres]
```

Circular dependencies are detected at config validation time.

### Per-service environment variables

Use the `[services.<name>.env]` sub-table for service-specific variables.
These are merged with (and override) global `[env]` and auto-generated
`DEVRIG_*` variables:

```toml
[services.api.env]
API_KEY = "secret"
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
```

## `[docker.*]` section

Each `[docker.<name>]` block defines a Docker container that devrig manages.
Containers are automatically pulled, started, health-checked, and initialized.

### Minimal docker

```toml
[docker.redis]
image = "redis:7-alpine"
port = 6379
ready_check = { type = "tcp" }
```

### Full docker

```toml
[docker.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
depends_on = []

[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"

[docker.postgres.ready_check]
type = "pg_isready"
```

### Docker fields

| Field           | Type               | Required | Default | Description                                   |
|-----------------|--------------------|----------|---------|-----------------------------------------------|
| `image`         | string             | Yes      | --      | Docker image (e.g. `postgres:16-alpine`).     |
| `port`          | integer or `"auto"`| No       | (none)  | Single port mapping (host:container).         |
| `ports`         | map of ports       | No       | `{}`    | Named port mappings for multi-port services.  |
| `env`           | map of strings     | No       | `{}`    | Container environment variables.              |
| `volumes`       | list of strings    | No       | `[]`    | Volume mounts (named `"vol:/path"` or bind `"/host:/path"`). |
| `command`       | string or list     | No       | (none)  | Override the image CMD.                        |
| `entrypoint`    | string or list     | No       | (none)  | Override the image ENTRYPOINT.                 |
| `ready_check`   | table              | No       | (none)  | Health check configuration.                   |
| `init`          | list of strings    | No       | `[]`    | SQL/commands to run after first ready.         |
| `depends_on`    | list of strings    | No       | `[]`    | Other docker or compose dependencies.          |
| `registry_auth` | table              | No       | (none)  | Registry credentials for private images.       |

### Port values for docker

Docker ports work the same as service ports:

```toml
port = 5432       # Fixed port. Container port is mapped to this host port.
port = "auto"     # devrig assigns a free ephemeral port and maps it.
```

Auto-assigned ports are sticky across restarts.

### Named ports

For services that expose multiple ports (like Mailpit with SMTP and UI):

```toml
[docker.mailpit]
image = "axllent/mailpit:latest"
[docker.mailpit.ports]
smtp = 1025
ui = 8025
```

Named ports generate additional environment variables:
- `DEVRIG_MAILPIT_PORT_SMTP=1025`
- `DEVRIG_MAILPIT_PORT_UI=8025`

### Ready check types

Ready checks verify that a container is ready to accept connections before
dependents are started.

```toml
# PostgreSQL pg_isready (runs inside container)
ready_check = { type = "pg_isready" }

# Custom command inside container
[docker.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"                # Optional: match this string in stdout

# HTTP health check (from host)
ready_check = { type = "http", url = "http://localhost:9000/health" }

# TCP port check (from host, uses the docker port)
ready_check = { type = "tcp" }

# Wait for a log pattern in container output
[docker.es.ready_check]
type = "log"
match = "started"
```

| Type         | Runs where | Description                                    | Timeout |
|--------------|------------|------------------------------------------------|---------|
| `pg_isready` | container  | Runs `pg_isready -h localhost -q -t 2`         | 30s     |
| `cmd`        | container  | Runs a command, checks exit code and stdout    | 30s     |
| `http`       | host       | GET request, checks for 2xx status             | 30s     |
| `tcp`        | host       | TCP connection to host port                    | 30s     |
| `log`        | container  | Streams logs and searches for pattern match    | 60s     |

All strategies use exponential backoff with jitter (250ms to 3s delay).

### Init scripts

Init scripts run inside the container after the ready check passes. They
only run on the first start. Use `devrig reset <docker>` to re-run them.

```toml
[docker.postgres]
image = "postgres:16-alpine"
init = [
    "CREATE DATABASE myapp;",
    "CREATE TABLE users (id serial PRIMARY KEY, name text);"
]
```

For postgres images, init scripts are executed via `psql -U <POSTGRES_USER> -c`.
For other images, they are executed via `sh -c`.

### Volumes

Volumes use the format `"name:/container/path"`. The volume name is
automatically scoped to the project (e.g. `devrig-myapp-a1b2c3d4-pgdata`).

```toml
volumes = ["pgdata:/var/lib/postgresql/data"]
```

Volumes persist across `devrig stop` but are removed by `devrig delete`.

#### Bind mounts

To mount a host directory directly into the container, use an absolute or
relative path as the source:

```toml
volumes = [
    "/home/user/data:/var/lib/postgresql/data",   # absolute path
    "./config:/etc/myapp",                         # relative to config file
    "../shared:/app/shared",                       # parent-relative
]
```

Bind mounts are detected when the source starts with `/`, `./`, or `../`.
No Docker volume is created for bind mounts — the host path is passed
directly to Docker. Bind mounts are **not** removed by `devrig delete`.

#### Command and entrypoint

Override the image's default `CMD` and/or `ENTRYPOINT`. Both accept a string
or a list of strings:

```toml
# Override CMD only — pass flags to the image's default entrypoint
[docker.redis]
image = "redis:7-alpine"
command = ["redis-server", "--appendonly", "yes"]

# Override ENTRYPOINT and CMD together
[docker.worker]
image = "python:3.12-slim"
entrypoint = ["python", "-u"]
command = ["worker.py", "--verbose"]

# String form (single element)
[docker.app]
image = "myapp:latest"
entrypoint = "/custom-entrypoint.sh"
```

When `entrypoint` is set, `command` provides the default arguments (matching
Docker semantics).

#### Docker vs Compose — when to use which

Use `[docker.*]` blocks when you want devrig to fully manage the container
lifecycle (pull, start, health-check, init, volumes, teardown). This is the
recommended approach for most infrastructure services.

Use `[compose]` when you have an existing `docker-compose.yml` that already
works and you want to integrate those services into devrig's dependency
graph without rewriting the config. Compose is a good stepping stone for
gradual migration to native `[docker.*]` blocks.

## `[cluster]` section

The optional `[cluster]` section defines a local Kubernetes cluster managed
by k3d. When present, `devrig cluster create` provisions the cluster,
builds images, and deploys services.

### Minimal cluster

```toml
[cluster]
```

This creates a cluster with the default name `devrig-{slug}`, one agent
node, and a local container registry.

### Full cluster

```toml
[cluster]
name = "myapp-dev"
agents = 2
ports = ["8080:80", "8443:443"]
registry = true
```

### Cluster fields

| Field      | Type            | Required | Default         | Description                                       |
|------------|-----------------|----------|-----------------|---------------------------------------------------|
| `name`     | string          | No       | `devrig-{slug}` | k3d cluster name.                                 |
| `agents`   | integer         | No       | `1`             | Number of k3d agent nodes.                        |
| `ports`    | list of strings | No       | `[]`            | Port mappings from host to cluster load balancer.  |
| `registry` | boolean         | No       | `true`          | Whether to create a local container registry.      |

Port mappings use the format `"hostPort:containerPort"`. The host port is
bound on `localhost` and forwarded through the k3d load balancer.

## `[cluster.deploy.*]` section

Each `[cluster.deploy.<name>]` block defines a containerized service to
build and deploy into the cluster.

### Minimal deploy

```toml
[cluster.deploy.api]
context = "./services/api"
manifests = ["k8s/deployment.yaml", "k8s/service.yaml"]
```

### Full deploy

```toml
[cluster.deploy.api]
context = "./services/api"
dockerfile = "Dockerfile"
manifests = ["k8s/deployment.yaml", "k8s/service.yaml"]
watch = true
depends_on = ["postgres"]
```

### Deploy fields

| Field        | Type            | Required | Default      | Description                                            |
|--------------|-----------------|----------|--------------|--------------------------------------------------------|
| `context`    | string          | Yes      | --           | Docker build context directory, relative to config.    |
| `dockerfile` | string          | No       | `Dockerfile` | Dockerfile path, relative to context.                  |
| `manifests`  | list of strings | Yes      | --           | Kubernetes manifest files to apply, relative to config.|
| `watch`      | boolean         | No       | `false`      | Enable file watching for automatic rebuild/redeploy.   |
| `depends_on` | list of strings | No       | `[]`         | Docker or other deploy services to start before this.   |

When `watch = true`, devrig monitors the build context directory for changes,
debounces with a 500ms window, rebuilds the Docker image, pushes it to the
local registry, and triggers a rollout restart. The directories `.git`,
`node_modules`, `target`, `__pycache__`, and `.devrig` are ignored.

## `[cluster.image.*]` section

Each `[cluster.image.<name>]` block defines a Docker image to build and push
to the cluster registry **without** applying any Kubernetes manifests or
creating a running deployment. Use this for images referenced by Jobs,
CronJobs, init containers, or any resource that doesn't need a persistent
Deployment.

### Minimal image

```toml
[cluster.image.job-runner]
context = "./tools/job-runner"
```

### Full image

```toml
[cluster.image.job-runner]
context = "./tools/job-runner"
dockerfile = "Dockerfile"
watch = true
depends_on = ["postgres"]
```

### Image fields

| Field        | Type            | Required | Default      | Description                                            |
|--------------|-----------------|----------|--------------|--------------------------------------------------------|
| `context`    | string          | Yes      | --           | Docker build context directory, relative to config.    |
| `dockerfile` | string          | No       | `Dockerfile` | Dockerfile path, relative to context.                  |
| `watch`      | boolean         | No       | `false`      | Enable file watching for automatic rebuild+push.       |
| `depends_on` | list of strings | No       | `[]`         | Docker, image, or deploy services to start before this.|

When `watch = true`, devrig monitors the build context directory for changes,
debounces with a 500ms window, and rebuilds+pushes the image. No rollout
restart is triggered since there is no Deployment. The same directories as
`cluster.deploy` are ignored (`.git`, `node_modules`, `target`, etc.).

Deploy entries can depend on image entries to ensure the image is available
in the registry before the deploy's manifests are applied:

```toml
[cluster.image.job-runner]
context = "./tools/job-runner"
watch = true

[cluster.deploy.api]
context = "./services/api"
manifests = "k8s/api"
watch = true
depends_on = ["job-runner"]   # ensures image exists before api deploys
```

## `[cluster.addons.*]` section

Addons are Helm charts, raw manifests, or Kustomize overlays that devrig
installs into the cluster after it is created but before services are deployed.
Use addons for shared infrastructure like ingress controllers, cert-manager,
or monitoring stacks.

### Helm addon (remote chart)

```toml
[cluster.addons.traefik]
type = "helm"
chart = "traefik/traefik"
repo = "https://traefik.github.io/charts"
namespace = "traefik"
version = "26.0.0"

[cluster.addons.traefik.values]
deployment.replicas = 1

[cluster.addons.traefik.port_forward]
8080 = "svc/traefik:80"
```

### Helm addon (local chart)

When `repo` is omitted, devrig treats `chart` as a local filesystem path
(relative to the config file):

```toml
[cluster.addons.myapp]
type = "helm"
chart = "./charts/myapp"
namespace = "myapp"
values_files = ["charts/myapp/values-dev.yaml"]

[cluster.addons.myapp.values]
"image.tag" = "dev"
```

### Manifest addon

```toml
[cluster.addons.monitoring]
type = "manifest"
path = "k8s/monitoring.yaml"
namespace = "monitoring"
```

### Kustomize addon

```toml
[cluster.addons.platform]
type = "kustomize"
path = "k8s/overlays/dev"
namespace = "platform"
```

### Addon fields

**Helm addons** (`type = "helm"`):

| Field          | Type           | Required | Default | Description                                         |
|----------------|----------------|----------|---------|-----------------------------------------------------|
| `type`         | string         | Yes      | --      | Must be `"helm"`.                                   |
| `chart`        | string         | Yes      | --      | Chart reference (`repo/chart`) or local path.       |
| `repo`         | string         | No       | (none)  | Helm repository URL. Omit for local charts.         |
| `namespace`    | string         | Yes      | --      | Kubernetes namespace for the release.               |
| `version`      | string         | No       | (latest)| Chart version constraint.                           |
| `values`       | map            | No       | `{}`    | Values passed via `helm --set`.                     |
| `values_files` | list           | No       | `[]`    | Values files passed via `helm -f`. Relative to config. |
| `port_forward` | map            | No       | `{}`    | Local port-forwards (see below).                    |

**Manifest addons** (`type = "manifest"`):

| Field          | Type           | Required | Default   | Description                              |
|----------------|----------------|----------|-----------|------------------------------------------|
| `type`         | string         | Yes      | --        | Must be `"manifest"`.                    |
| `path`         | string         | Yes      | --        | Path to YAML manifest, relative to config.|
| `namespace`    | string         | No       | `default` | Namespace for `kubectl apply`.           |
| `port_forward` | map            | No       | `{}`      | Local port-forwards (see below).         |

**Kustomize addons** (`type = "kustomize"`):

| Field          | Type           | Required | Default   | Description                              |
|----------------|----------------|----------|-----------|------------------------------------------|
| `type`         | string         | Yes      | --        | Must be `"kustomize"`.                   |
| `path`         | string         | Yes      | --        | Path to kustomization directory.         |
| `namespace`    | string         | No       | `default` | Namespace for `kubectl apply -k`.        |
| `port_forward` | map            | No       | `{}`      | Local port-forwards (see below).         |

### Port forwarding

Any addon can declare port-forwards that devrig starts automatically after
the addon is installed:

```toml
[cluster.addons.grafana.port_forward]
3000 = "svc/grafana:80"
9090 = "svc/prometheus-server:80"
```

The key is the local port and the value is a `kubectl port-forward` target
in the format `resource:remotePort`. Port-forwards automatically reconnect
with exponential backoff if the connection drops.

### Lifecycle

- `devrig start` installs addons in alphabetical order after the cluster is
  created and before services are deployed. Installation is idempotent
  (`helm upgrade --install`).
- `devrig delete` uninstalls addons in reverse alphabetical order before
  deleting the cluster.
- Addons appear in the startup summary as `[addon] name`.

## `[compose]` section

The `[compose]` section delegates infrastructure to an existing
`docker-compose.yml`. This enables incremental migration from docker-compose
to native `[docker.*]` blocks.

```toml
[compose]
file = "docker-compose.yml"
services = ["redis", "postgres"]    # Which services to start (empty = auto-discover from file)
env_file = ".env"                   # Optional env file for compose

[compose.ready_checks.redis]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"
```

### Compose fields

| Field          | Type            | Required | Default | Description                             |
|----------------|-----------------|----------|---------|-----------------------------------------|
| `file`         | string          | Yes      | --      | Path to docker-compose.yml              |
| `services`     | list of strings | No       | `[]`    | Services to start (auto-discovered from compose file if empty) |
| `env_file`     | string          | No       | (none)  | Env file to pass to `docker compose up` |
| `ready_checks` | map of checks   | No       | `{}`    | Ready checks for compose services       |

Compose services participate in the dependency graph — local services can
list compose service names in `depends_on`. When `services` is empty or
omitted, devrig auto-discovers service names from the docker-compose file,
so you don't need to list them explicitly just to use them as dependencies.

### Lifecycle

- `devrig start` runs `docker compose up -d` and connects containers to the
  devrig network
- `devrig stop` leaves compose containers running (managed by compose)
- `devrig delete` runs `docker compose down --remove-orphans`

## Dashboard

The optional `[dashboard]` section enables the built-in web dashboard and
OpenTelemetry collector. When present, `devrig start` launches a local
dashboard server and OTLP receivers that collect traces, logs, and metrics
from your services.

All dashboard and OTel ports auto-resolve if already in use, so multiple
devrig instances can run simultaneously without port conflicts.

### `[dashboard]` section

```toml
[dashboard]
port = 4000
enabled = true
```

| Field     | Type    | Default | Description                                |
|-----------|---------|---------|--------------------------------------------|
| `port`    | integer | `4000`  | HTTP port for the dashboard web UI and API |
| `enabled` | boolean | `true`  | Whether to start the dashboard             |

When `enabled` is omitted or set to `true`, the dashboard starts
automatically with `devrig start`. Set `enabled = false` to disable the
dashboard while keeping the configuration in place.

### `[dashboard.otel]` section

The `[dashboard.otel]` sub-section configures the OpenTelemetry collector
that receives telemetry from your services:

```toml
[dashboard.otel]
grpc_port = 4317
http_port = 4318
trace_buffer = 10000
metric_buffer = 50000
log_buffer = 100000
retention = "1h"
```

| Field          | Type    | Default  | Description                                  |
|----------------|---------|----------|----------------------------------------------|
| `grpc_port`    | integer | `4317`   | OTLP gRPC receiver port                      |
| `http_port`    | integer | `4318`   | OTLP HTTP receiver port                      |
| `trace_buffer` | integer | `10000`  | Maximum number of spans stored in memory      |
| `metric_buffer`| integer | `50000`  | Maximum number of metric data points stored   |
| `log_buffer`   | integer | `100000` | Maximum number of log records stored           |
| `retention`    | string  | `"1h"`   | How long to keep telemetry data (e.g. `"1h"`, `"30m"`, `"2h30m"`) |

The `retention` field accepts any duration string supported by the
`humantime` crate. Telemetry older than the retention period is
automatically swept from memory every 30 seconds. If the buffer fills
before the retention period, the oldest entries are evicted first.

### Auto-injected environment variables

When the dashboard is enabled, devrig automatically injects the following
environment variables into every service process:

| Variable                       | Example value                      | Description                          |
|--------------------------------|------------------------------------|--------------------------------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317`            | OTLP gRPC endpoint for SDK auto-config |
| `OTEL_SERVICE_NAME`            | `api`                              | Set to the service name from config  |
| `DEVRIG_DASHBOARD_URL`         | `http://localhost:4000`            | Dashboard URL for reference          |

These variables allow OpenTelemetry SDKs to auto-discover the collector
without manual configuration. Most OTLP-compatible libraries (Go, Python,
Node.js, Rust) will automatically send telemetry to the endpoint specified
by `OTEL_EXPORTER_OTLP_ENDPOINT`.

### Dashboard configuration example

```toml
[project]
name = "myapp"

[dashboard]
port = 4000
enabled = true

[dashboard.otel]
grpc_port = 4317
http_port = 4318
trace_buffer = 20000
metric_buffer = 100000
log_buffer = 200000
retention = "2h"

[docker.postgres]
image = "postgres:16-alpine"
port = 5432
[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
[docker.postgres.ready_check]
type = "pg_isready"

[services.api]
path = "./api"
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres"]
[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
```

With this configuration, `devrig start` will:

1. Start Postgres as a Docker container.
2. Launch the dashboard on `http://localhost:4000`.
3. Start OTLP receivers on ports 4317 (gRPC) and 4318 (HTTP).
4. Start the `api` service with `OTEL_EXPORTER_OTLP_ENDPOINT`,
   `OTEL_SERVICE_NAME`, and `DEVRIG_DASHBOARD_URL` injected automatically.
5. Telemetry from the API service will appear in the dashboard and be
   queryable via `devrig query` commands.

## Environment variable expansion

Any string value in `env`, `docker.*.env`, `docker.*.image`,
`docker.*.registry_auth`, or `cluster.registries` can reference environment
variables using `$VAR` or `${VAR}` syntax:

```toml
[env]
SECRET_KEY = "$MY_SECRET_KEY"

[services.api.env]
DATABASE_URL = "postgres://user:${DB_PASS}@localhost:{{ docker.postgres.port }}/mydb"
```

### Expansion pipeline

Expansion runs after TOML parsing but **before** validation and template
interpolation:

```
Parse TOML → Load .env files → Expand $VAR/${VAR} → Validate → Resolve {{ }} templates
```

This means `$DB_PASS` is expanded first, then `{{ docker.postgres.port }}`
is resolved later, so both can coexist in the same value.

### Lookup order

1. Values from `.env` files (project-level and per-service)
2. Host process environment (`std::env::var`)

### Escaping

Use `$$` to produce a literal `$` in the output:

```toml
PRICE = "$$5.00"   # becomes: $5.00
```

### Error handling

Undefined variables produce a clear error with the field path:

```
Error: undefined environment variable $DB_PASS in services.api.env.DATABASE_URL
```

## `.env` file support

devrig can load `.env` files at two levels:

```toml
[project]
name = "myapp"
env_file = ".env"              # project-wide secrets

[services.api]
command = "cargo run"
env_file = ".env.api"          # per-service overrides
```

### File format

```env
# Comments and blank lines are ignored
DATABASE_URL=postgres://localhost/mydb
SECRET_KEY="quoted values work"
API_TOKEN='single quotes too'
```

### Merge priority (lowest to highest)

1. Project-level `.env` file
2. Per-service `.env` file
3. Explicit TOML `[env]` or `[services.*.env]` values

Explicit TOML values always win over `.env` file values.

## Docker registry authentication

Pull images from private registries by adding `registry_auth`:

```toml
[docker.my-app]
image = "ghcr.io/org/app:latest"
registry_auth = { username = "$REGISTRY_USER", password = "$REGISTRY_TOKEN" }
```

Credentials support `$VAR` expansion, so secrets stay out of the TOML file.
The username and password are passed to the Docker daemon as
`DockerCredentials` during `docker pull`.

## k3d cluster registry authentication

Configure private registry access for the k3d cluster with
`[[cluster.registries]]`:

```toml
[cluster]
registry = true

[[cluster.registries]]
url = "ghcr.io"
username = "$GH_USER"
password = "$GH_TOKEN"

[[cluster.registries]]
url = "docker.io"
username = "$DOCKER_USER"
password = "$DOCKER_TOKEN"
```

| Field      | Type   | Required | Description                        |
|------------|--------|----------|------------------------------------|
| `url`      | string | Yes      | Registry hostname                  |
| `username` | string | Yes      | Registry username (supports `$VAR`)|
| `password` | string | Yes      | Registry password (supports `$VAR`)|

devrig generates a `registries.yaml` file and passes it to `k3d cluster
create --registry-config`. This allows pods in the cluster to pull from
private registries.

## Secret masking

When `devrig env <service>` prints environment variables, any values that
were produced by `$VAR` expansion are automatically masked with `****`.
This prevents accidentally leaking secrets in terminal output or logs.

```bash
$ devrig env api
DATABASE_URL=postgres://user:****@localhost:5432/mydb
API_KEY=****
RUST_LOG=debug
```

## `[env]` section

The optional `[env]` section defines environment variables that are passed
to **every** service:

```toml
[env]
RUST_LOG = "debug"
NODE_ENV = "development"
```

Per-service `env` values override global `env` values with the same key.

## `[network]` section

Optional custom Docker network configuration:

```toml
[network]
name = "custom-network-name"
```

If omitted, devrig creates a network named `devrig-{slug}-net`.

## Template expressions

Service env values support `{{ dotted.path }}` template expressions that
resolve to values from the config and resolved ports:

```toml
[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"
SMTP_PORT = "{{ docker.mailpit.ports.smtp }}"
APP_NAME = "{{ project.name }}"
```

### Available template variables

| Variable                       | Example value |
|--------------------------------|---------------|
| `project.name`                 | `myapp`       |
| `services.<name>.port`         | `3000`        |
| `docker.<name>.port`            | `5432`        |
| `docker.<name>.ports.<portname>`| `1025`        |
| `compose.<name>.port`          | `6379`        |
| `cluster.name`                 | `myapp-dev`   |

The `cluster.name` variable is available when a `[cluster]` section is
defined. It resolves to the cluster name and is useful in Kubernetes
manifests for referencing images in the local registry:

```yaml
image: k3d-{{ cluster.name }}-reg:5000/api:latest
```

Templates are resolved after all ports are assigned (Phase 4 of startup).
Unresolved references produce an error before any services are started.

## Service discovery (`DEVRIG_*` variables)

Every service process automatically receives environment variables for
service discovery. These are generated in this order (later overrides earlier):

1. Global `[env]`
2. Auto-generated `DEVRIG_*` vars for all docker services
3. Auto-generated `DEVRIG_*` vars for all other services
4. Service's own `PORT` and `HOST`
5. Service-specific `[services.<name>.env]` (can override any of the above)

### Docker container variables

For each `[docker.<name>]`, all services receive:

| Variable                         | Example                                      |
|----------------------------------|----------------------------------------------|
| `DEVRIG_<NAME>_HOST`             | `DEVRIG_POSTGRES_HOST=localhost`              |
| `DEVRIG_<NAME>_PORT`             | `DEVRIG_POSTGRES_PORT=5432`                   |
| `DEVRIG_<NAME>_URL`              | `DEVRIG_POSTGRES_URL=postgres://u:p@...:5432` |
| `DEVRIG_<NAME>_PORT_<PORTNAME>`  | `DEVRIG_MAILPIT_PORT_SMTP=1025`               |

### URL generation

URLs are generated based on the Docker image name:

| Image prefix | URL format                                 |
|-------------|---------------------------------------------|
| `postgres`  | `postgres://user:pass@localhost:port`       |
| `redis`     | `redis://localhost:port`                    |
| Multi-port  | `localhost:port` (no protocol)              |
| Default     | `http://localhost:port`                     |

Postgres credentials are extracted from `POSTGRES_USER` and `POSTGRES_PASSWORD`
in the docker env.

### Service-to-service variables

Each service sees `DEVRIG_*` vars for every *other* service:

| Variable               | Example                          |
|------------------------|----------------------------------|
| `DEVRIG_<NAME>_HOST`   | `DEVRIG_API_HOST=localhost`      |
| `DEVRIG_<NAME>_PORT`   | `DEVRIG_API_PORT=3000`           |
| `DEVRIG_<NAME>_URL`    | `DEVRIG_API_URL=http://localhost:3000` |

A service does NOT see its own `DEVRIG_*` vars. Instead it gets `PORT` and
`HOST` for itself.

### Inspecting variables

Use `devrig env <service>` to see the full resolved environment:

```bash
devrig env api
```

## CLI commands

### `devrig start [services...]`

Start all services, or only the named services plus their transitive
dependencies.

### `devrig stop`

Stop all running services and docker containers. Preserves state for restart.

### `devrig delete`

Stop everything and remove all Docker resources (containers, volumes,
networks) and state files.

### `devrig ps [--all]`

Show running services and their status. `--all` shows all known devrig
instances across projects.

### `devrig env <service>`

Print the resolved environment variables for a service.

### `devrig exec <docker> -- <command...>`

Execute a command inside a docker container.

### `devrig reset <docker>`

Clear the init-completed flag for a docker service. Init scripts will
re-run on the next `devrig start`.

### `devrig cluster create`

Create the k3d cluster, local registry, build all deploy images, and apply
all Kubernetes manifests. If file watchers are configured (`watch = true`),
they start automatically.

```bash
devrig cluster create
```

### `devrig cluster delete`

Tear down the k3d cluster, registry, and remove the local kubeconfig.

```bash
devrig cluster delete
```

### `devrig cluster kubeconfig`

Print the absolute path to the project-local kubeconfig file. Useful for
exporting:

```bash
export KUBECONFIG=$(devrig cluster kubeconfig)
```

### `devrig kubectl` / `devrig k`

Run kubectl commands against the devrig cluster with the correct kubeconfig
set automatically. `devrig k` is a short alias.

```bash
devrig kubectl get pods
devrig k logs -f deployment/api
devrig k exec -it deployment/api -- sh
```

### `devrig doctor`

Check that required tools (Docker, k3d, kubectl, etc.) are installed and
running.

### `devrig init`

Generate a starter `devrig.toml` based on project type detection.

### `devrig validate`

Validate the configuration file and report errors with rich diagnostics.
Uses rustc-style error messages with source spans, labels, and "did you
mean?" suggestions for typos.

```bash
devrig validate
devrig validate -f devrig.staging.toml
```

### `devrig skill install [--global]`

Install the Claude Code skill file for AI-assisted debugging.

```bash
devrig skill install           # Install to project .claude/skills/
devrig skill install --global  # Install to ~/.claude/skills/
```

See the [Claude Code Skill guide](claude-code-skill.md) for details.

### `devrig logs [services...] [options]`

Show and filter service logs from the JSONL log file.

```bash
devrig logs                         # All logs
devrig logs api web                 # Only api and web
devrig logs --tail 100              # Last 100 lines
devrig logs --since 5m              # Last 5 minutes
devrig logs --grep "ERROR"          # Lines matching regex
devrig logs --exclude "health"      # Exclude lines matching regex
devrig logs --level warn            # Minimum log level
devrig logs --format json           # Output as JSONL
devrig logs -o logs.txt             # Write to file
devrig logs -t                      # Show timestamps
```

| Flag          | Short | Description                                     |
|---------------|-------|-------------------------------------------------|
| `--follow`    | `-F`  | Follow log output (live tail)                   |
| `--tail N`    |       | Show last N lines                               |
| `--since`     |       | Show logs since duration (e.g. `5m`, `1h`, `30s`) |
| `--grep`      | `-g`  | Include only lines matching regex                |
| `--exclude`   | `-v`  | Exclude lines matching regex                     |
| `--level`     | `-l`  | Minimum log level (trace, debug, info, warn, error) |
| `--format`    |       | Output format: `text` (default) or `json`        |
| `--output`    | `-o`  | Write output to file                             |
| `--timestamps`| `-t`  | Show timestamps in output                        |

### `devrig completions <shell>`

Generate shell completions for bash, zsh, fish, elvish, or powershell.

```bash
# Bash
devrig completions bash > ~/.local/share/bash-completion/completions/devrig

# Zsh
devrig completions zsh > ~/.zfunc/_devrig

# Fish
devrig completions fish > ~/.config/fish/completions/devrig.fish
```

## Complete example

```toml
[project]
name = "myapp"

[env]
RUST_LOG = "debug"

[docker.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
[docker.postgres.ready_check]
type = "pg_isready"

[docker.redis]
image = "redis:7-alpine"
port = 6379
[docker.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"

[services.api]
path = "./api"
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres", "redis"]
[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ docker.postgres.port }}/myapp"

[services.web]
path = "./web"
command = "npm run dev"
port = "auto"
depends_on = ["api"]
```

## Complete example with cluster

This example shows a project that runs Postgres as a local docker container and deploys
an API service into a local Kubernetes cluster:

```toml
[project]
name = "myapp"

[docker.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
[docker.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
[docker.postgres.ready_check]
type = "pg_isready"

[cluster]
name = "myapp-dev"
agents = 1
ports = ["8080:80"]

[cluster.deploy.api]
context = "./services/api"
dockerfile = "Dockerfile"
manifests = ["k8s/deployment.yaml", "k8s/service.yaml"]
watch = true
depends_on = ["postgres"]
```

With this configuration:

- `devrig start` launches Postgres as a Docker container.
- `devrig cluster create` creates the k3d cluster, builds the API image,
  pushes it to the local registry, applies the manifests, and watches for
  file changes.
- Pods connect to Postgres via the Docker container name on the shared network.
- The API is accessible from the host at `http://localhost:8080`.

## Validation rules

devrig validates the configuration before starting any services. Errors are
displayed with rich diagnostics powered by miette, including source spans,
labels, and "did you mean?" suggestions for typos.

Run `devrig validate` to check your config without starting services.

1. **`[project]` is required** -- The file must contain a `[project]` section
   with a `name` field.
2. **`command` is required** -- Every service must have a non-empty `command`.
3. **`image` is required** -- Every docker container must have a non-empty `image`.
4. **Dependencies must exist** -- Every entry in `depends_on` must reference
   a defined service, docker, or compose service name. Typos trigger a "did
   you mean?" suggestion if a close match exists.
5. **No duplicate ports** -- Two services or docker containers cannot declare the same
   fixed port.
6. **No cycles** -- The dependency graph must be acyclic.
7. **Compose file is non-empty** -- If `[compose]` is present, `file` must
   be specified.
8. **Restart policy is valid** -- If `[services.<name>.restart]` is present,
   `policy` must be one of `always`, `on-failure`, or `never`.
9. **Addon charts are non-empty** -- Helm addons must have a non-empty `chart`.
   If `repo` is provided, it must also be non-empty.
10. **Image context is non-empty** -- Cluster image entries must have a
    non-empty `context`.
11. **Image names are unique** -- Cluster image names must not conflict with
    cluster deploy names or other resource types.
12. **Addon names are unique** -- Addon names must not conflict with cluster
    deploy names.
13. **Addon ports are unique** -- Port-forward local ports must not conflict
    with service ports.

All validation errors are reported together so you can fix them in one pass.
