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
```

| Field  | Type   | Required | Description                                        |
|--------|--------|----------|----------------------------------------------------|
| `name` | string | Yes      | Project name. Used in the slug and display output.  |

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
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"
```

### Fields

| Field        | Type               | Required | Default | Description                                              |
|--------------|--------------------|----------|---------|----------------------------------------------------------|
| `command`    | string             | Yes      | --      | Shell command to run (executed via `sh -c`).              |
| `path`       | string             | No       | (none)  | Working directory, relative to the config file.           |
| `port`       | integer or `"auto"`| No       | (none)  | Port the service listens on.                              |
| `env`        | map of strings     | No       | `{}`    | Environment variables for this service.                   |
| `depends_on` | list of strings    | No       | `[]`    | Services, infra, or compose services to start before this.|

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

### Dependencies

The `depends_on` list controls startup order. Dependencies can reference
service names, infra names, or compose service names:

```toml
[services.api]
command = "cargo run"
depends_on = ["postgres", "redis"]  # postgres is [infra.postgres]
```

Circular dependencies are detected at config validation time.

### Per-service environment variables

Use the `[services.<name>.env]` sub-table for service-specific variables.
These are merged with (and override) global `[env]` and auto-generated
`DEVRIG_*` variables:

```toml
[services.api.env]
API_KEY = "secret"
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"
```

## `[infra.*]` section

Each `[infra.<name>]` block defines a Docker container that devrig manages.
Containers are automatically pulled, started, health-checked, and initialized.

### Minimal infra

```toml
[infra.redis]
image = "redis:7-alpine"
port = 6379
ready_check = { type = "tcp" }
```

### Full infra

```toml
[infra.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
depends_on = []

[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"

[infra.postgres.ready_check]
type = "pg_isready"
```

### Infra fields

| Field         | Type               | Required | Default | Description                                   |
|---------------|--------------------|----------|---------|-----------------------------------------------|
| `image`       | string             | Yes      | --      | Docker image (e.g. `postgres:16-alpine`).     |
| `port`        | integer or `"auto"`| No       | (none)  | Single port mapping (host:container).         |
| `ports`       | map of ports       | No       | `{}`    | Named port mappings for multi-port services.  |
| `env`         | map of strings     | No       | `{}`    | Container environment variables.              |
| `volumes`     | list of strings    | No       | `[]`    | Volume mounts (`"name:/path/in/container"`).  |
| `ready_check` | table              | No       | (none)  | Health check configuration.                   |
| `init`        | list of strings    | No       | `[]`    | SQL/commands to run after first ready.         |
| `depends_on`  | list of strings    | No       | `[]`    | Other infra or compose dependencies.          |

### Port values for infra

Infra ports work the same as service ports:

```toml
port = 5432       # Fixed port. Container port is mapped to this host port.
port = "auto"     # devrig assigns a free ephemeral port and maps it.
```

Auto-assigned ports are sticky across restarts.

### Named ports

For services that expose multiple ports (like Mailpit with SMTP and UI):

```toml
[infra.mailpit]
image = "axllent/mailpit:latest"
[infra.mailpit.ports]
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
[infra.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"                # Optional: match this string in stdout

# HTTP health check (from host)
ready_check = { type = "http", url = "http://localhost:9000/health" }

# TCP port check (from host, uses the infra port)
ready_check = { type = "tcp" }

# Wait for a log pattern in container output
[infra.es.ready_check]
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
only run on the first start. Use `devrig reset <infra>` to re-run them.

```toml
[infra.postgres]
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

## `[compose]` section

The `[compose]` section delegates infrastructure to an existing
`docker-compose.yml`. This enables incremental migration from docker-compose
to native `[infra.*]` blocks.

```toml
[compose]
file = "docker-compose.yml"
services = ["redis", "postgres"]    # Which services to start (empty = all)
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
| `services`     | list of strings | No       | `[]`    | Specific services to start (all if empty)|
| `env_file`     | string          | No       | (none)  | Env file to pass to `docker compose up` |
| `ready_checks` | map of checks   | No       | `{}`    | Ready checks for compose services       |

Compose services participate in the dependency graph -- local services can
list compose service names in `depends_on`.

### Lifecycle

- `devrig start` runs `docker compose up -d` and connects containers to the
  devrig network
- `devrig stop` leaves compose containers running (managed by compose)
- `devrig delete` runs `docker compose down --remove-orphans`

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
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"
SMTP_PORT = "{{ infra.mailpit.ports.smtp }}"
APP_NAME = "{{ project.name }}"
```

### Available template variables

| Variable                       | Example value |
|--------------------------------|---------------|
| `project.name`                 | `myapp`       |
| `services.<name>.port`         | `3000`        |
| `infra.<name>.port`            | `5432`        |
| `infra.<name>.ports.<portname>`| `1025`        |
| `compose.<name>.port`          | `6379`        |

Templates are resolved after all ports are assigned (Phase 4 of startup).
Unresolved references produce an error before any services are started.

## Service discovery (`DEVRIG_*` variables)

Every service process automatically receives environment variables for
service discovery. These are generated in this order (later overrides earlier):

1. Global `[env]`
2. Auto-generated `DEVRIG_*` vars for all infra services
3. Auto-generated `DEVRIG_*` vars for all other services
4. Service's own `PORT` and `HOST`
5. Service-specific `[services.<name>.env]` (can override any of the above)

### Infrastructure variables

For each `[infra.<name>]`, all services receive:

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
in the infra env.

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

Stop all running services and infra containers. Preserves state for restart.

### `devrig delete`

Stop everything and remove all Docker resources (containers, volumes,
networks) and state files.

### `devrig ps [--all]`

Show running services and their status. `--all` shows all known devrig
instances across projects.

### `devrig env <service>`

Print the resolved environment variables for a service.

### `devrig exec <infra> -- <command...>`

Execute a command inside an infra container.

### `devrig reset <infra>`

Clear the init-completed flag for an infra service. Init scripts will
re-run on the next `devrig start`.

### `devrig doctor`

Check that required tools (Docker, etc.) are installed and running.

### `devrig init`

Generate a starter `devrig.toml` based on project type detection.

## Complete example

```toml
[project]
name = "myapp"

[env]
RUST_LOG = "debug"

[infra.postgres]
image = "postgres:16-alpine"
port = 5432
volumes = ["pgdata:/var/lib/postgresql/data"]
init = ["CREATE DATABASE myapp;"]
[infra.postgres.env]
POSTGRES_USER = "devrig"
POSTGRES_PASSWORD = "devrig"
[infra.postgres.ready_check]
type = "pg_isready"

[infra.redis]
image = "redis:7-alpine"
port = 6379
[infra.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"

[services.api]
path = "./api"
command = "cargo watch -x run"
port = 3000
depends_on = ["postgres", "redis"]
[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"

[services.web]
path = "./web"
command = "npm run dev"
port = "auto"
depends_on = ["api"]
```

## Validation rules

devrig validates the configuration before starting any services:

1. **`[project]` is required** -- The file must contain a `[project]` section
   with a `name` field.
2. **`command` is required** -- Every service must have a non-empty `command`.
3. **`image` is required** -- Every infra must have a non-empty `image`.
4. **Dependencies must exist** -- Every entry in `depends_on` must reference
   a defined service, infra, or compose service name.
5. **No duplicate ports** -- Two services or infra cannot declare the same
   fixed port.
6. **No cycles** -- The dependency graph must be acyclic.
7. **Compose file is non-empty** -- If `[compose]` is present, `file` must
   be specified.

All validation errors are reported together so you can fix them in one pass.
