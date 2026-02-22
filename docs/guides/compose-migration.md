# Compose Migration Guide

devrig can manage services defined in an existing `docker-compose.yml` alongside
native `[infra.*]` blocks and local process services. This enables incremental
migration from docker-compose to devrig-managed infrastructure.

## Configuration

Add a `[compose]` block to your `devrig.toml`:

```toml
[compose]
file = "docker-compose.yml"
services = ["redis", "postgres"]
env_file = ".env"                    # optional

[compose.ready_checks.redis]
type = "cmd"
command = "redis-cli ping"
expect = "PONG"
```

### Fields

| Field | Required | Description |
|---|---|---|
| `file` | Yes | Path to docker-compose.yml (relative to devrig.toml) |
| `services` | No | Specific services to start (all if empty) |
| `env_file` | No | Env file to pass to `docker compose up` |
| `ready_checks.<svc>` | No | Ready check for a compose service |

## How It Works

1. `devrig start` runs `docker compose up -d` for the listed services
2. Compose containers are bridged to the devrig Docker network
3. Ready checks run if configured
4. Compose service ports are discovered from `docker compose ps`
5. Services can depend on compose services via `depends_on`

## Service Dependencies

Services can depend on compose-managed services just like native infra:

```toml
[compose]
file = "docker-compose.yml"
services = ["redis"]

[services.api]
command = "cargo run"
port = 3000
depends_on = ["redis"]
```

## Migration Path

1. Start with all infrastructure in docker-compose.yml
2. Add `[compose]` to devrig.toml referencing your compose services
3. Gradually move services from compose to `[infra.*]` blocks
4. Remove compose services as they are replaced

## Lifecycle

- `devrig start` -> `docker compose up -d` for compose services
- `devrig stop` -> Compose services continue running (managed by compose)
- `devrig delete` -> `docker compose down --remove-orphans`
