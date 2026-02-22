# Configuration Reference

devrig is configured via a `devrig.toml` file in your project root.

## Project

```toml
[project]
name = "myapp"   # Required. Used for naming Docker resources.
```

## Services

Local process services managed by devrig:

```toml
[services.api]
path = "./api"              # Working directory (relative to devrig.toml)
command = "cargo watch -x run"
port = 3000                 # Fixed port, or "auto" for ephemeral
depends_on = ["postgres"]   # Wait for these before starting

[services.api.env]
API_KEY = "dev-secret"
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"
```

### Service Fields

| Field | Required | Default | Description |
|---|---|---|---|
| `command` | Yes | - | Shell command to run |
| `path` | No | config dir | Working directory |
| `port` | No | none | Port number or `"auto"` |
| `env` | No | `{}` | Environment variables |
| `depends_on` | No | `[]` | Dependencies (services, infra, or compose) |

## Infrastructure

Docker containers managed by devrig:

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

### Infrastructure Fields

| Field | Required | Default | Description |
|---|---|---|---|
| `image` | Yes | - | Docker image |
| `port` | No | none | Single port mapping (number or `"auto"`) |
| `ports` | No | `{}` | Named port mappings |
| `env` | No | `{}` | Container environment variables |
| `volumes` | No | `[]` | Volume mounts (`"name:/path"`) |
| `ready_check` | No | none | Health check configuration |
| `init` | No | `[]` | SQL/commands to run after first ready |
| `depends_on` | No | `[]` | Other infra/compose dependencies |

### Ready Check Types

```toml
# PostgreSQL pg_isready
ready_check = { type = "pg_isready" }

# Run a command inside the container
[infra.redis.ready_check]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"

# HTTP health check (from host)
ready_check = { type = "http", url = "http://localhost:9000/health" }

# TCP port check (from host)
ready_check = { type = "tcp" }

# Wait for log pattern
[infra.es.ready_check]
type = "log"
match = "started"
```

### Named Ports

For services that expose multiple ports:

```toml
[infra.mailpit]
image = "axllent/mailpit:latest"
[infra.mailpit.ports]
smtp = 1025
ui = 8025
```

## Compose

Delegate to an existing docker-compose.yml:

```toml
[compose]
file = "docker-compose.yml"
services = ["redis", "postgres"]
env_file = ".env"

[compose.ready_checks.redis]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"
```

## Global Environment

Shared environment variables injected into all services:

```toml
[env]
RUST_LOG = "debug"
NODE_ENV = "development"
```

## Network

Optional custom Docker network name:

```toml
[network]
name = "custom-network-name"
```

## Full Example

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
