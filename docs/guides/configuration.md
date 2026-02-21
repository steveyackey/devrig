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

Each `[services.<name>]` block defines a service that devrig manages. The
`<name>` is an arbitrary identifier used in logs, status output, and
`depends_on` references.

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
depends_on = ["db", "cache"]

[services.api.env]
API_KEY = "dev-secret"
LOG_LEVEL = "debug"
```

### Fields

| Field        | Type               | Required | Default | Description                                     |
|--------------|--------------------|----------|---------|-------------------------------------------------|
| `command`    | string             | Yes      | --      | Shell command to run (executed via `sh -c`).     |
| `path`       | string             | No       | (none)  | Working directory, relative to the config file.  |
| `port`       | integer or `"auto"`| No       | (none)  | Port the service listens on.                     |
| `env`        | map of strings     | No       | `{}`    | Environment variables for this service.          |
| `depends_on` | list of strings    | No       | `[]`    | Services that must start before this one.        |

### Port values

The `port` field accepts three forms:

```toml
port = 3000       # Fixed port. devrig verifies it is available at startup.
port = "auto"     # devrig assigns a free ephemeral port.
# (omitted)       # No port management. The service manages its own port.
```

When a port is specified (fixed or auto), devrig sets the `PORT` environment
variable so the service can discover its assigned port.

### Command execution

The `command` string is passed to `sh -c`, so shell features (pipes,
redirects, `&&` chains) work:

```toml
command = "cargo watch -x run"
command = "npm run dev -- --port $PORT"
command = "docker compose up postgres"
```

Each service runs in its own process group. On shutdown, SIGTERM is sent to
the entire group.

### Working directory

The `path` field is resolved relative to the config file location, not the
current working directory. If omitted, the service runs in the directory
containing `devrig.toml`.

```toml
# If devrig.toml is at /home/user/project/devrig.toml
# this service runs in /home/user/project/services/api
[services.api]
path = "./services/api"
command = "cargo watch -x run"
```

### Dependencies

The `depends_on` list controls startup order. Services are started in
topological order so that dependencies are running before their dependents.

```toml
[services.web]
command = "npm run dev"
depends_on = ["api"]

[services.api]
command = "cargo run"
depends_on = ["db"]

[services.db]
command = "docker compose up postgres"
```

Start order: `db`, `api`, `web`.

Circular dependencies are detected at config validation time and produce a
clear error message.

### Per-service environment variables

Use the `[services.<name>.env]` sub-table for service-specific variables.
These are merged with (and override) global `[env]` variables:

```toml
[services.api.env]
API_KEY = "secret"
DEBUG = "true"
```

## `[env]` section

The optional `[env]` section defines environment variables that are passed
to **every** service:

```toml
[env]
RUST_LOG = "debug"
DATABASE_URL = "postgres://localhost/myapp"
```

Per-service `env` values override global `env` values with the same key.

The merge order is:

1. Global `[env]`
2. Per-service `[services.<name>.env]` (overrides global)
3. `PORT` (set by devrig if a port is configured)

## Complete example

```toml
[project]
name = "myapp"

[env]
RUST_LOG = "debug"
DATABASE_URL = "postgres://devrig:devrig@localhost:5432/myapp"

[services.db]
command = "docker compose up postgres"
port = 5432

[services.cache]
command = "docker compose up redis"
port = 6379

[services.api]
path = "./services/api"
command = "cargo watch -x run"
port = 3000
depends_on = ["db", "cache"]

[services.api.env]
API_KEY = "dev-secret-key"

[services.worker]
path = "./services/worker"
command = "cargo watch -x run"
port = "auto"
depends_on = ["db"]

[services.web]
path = "./apps/web"
command = "npm run dev"
port = 5173
depends_on = ["api"]
```

## Validation rules

devrig validates the configuration before starting any services:

1. **`[project]` is required** -- The file must contain a `[project]` section
   with a `name` field.
2. **`command` is required** -- Every service must have a non-empty `command`.
3. **Dependencies must exist** -- Every entry in `depends_on` must reference
   a defined service.
4. **No duplicate ports** -- Two services cannot declare the same fixed port.
5. **No cycles** -- The dependency graph must be acyclic.

All validation errors are reported together so you can fix them in one pass.
