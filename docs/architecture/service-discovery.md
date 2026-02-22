# Service Discovery

devrig automatically generates environment variables so services can discover
each other and infrastructure without hardcoding connection strings.

## Environment Variable Layering

Variables are applied in this order (later overrides earlier):

1. Global env from `[env]`
2. Auto-generated `DEVRIG_*` vars for all infra services
3. Auto-generated `DEVRIG_*` vars for all other services
4. `PORT` and `HOST` for the service itself
5. Service-specific env from `[services.<name>.env]`

## Infrastructure Variables

For each `[infra.<name>]` block, all services receive:

| Variable | Example | Description |
|---|---|---|
| `DEVRIG_<NAME>_HOST` | `DEVRIG_POSTGRES_HOST=localhost` | Always `localhost` |
| `DEVRIG_<NAME>_PORT` | `DEVRIG_POSTGRES_PORT=5432` | Resolved port |
| `DEVRIG_<NAME>_URL` | `DEVRIG_POSTGRES_URL=postgres://user:pass@localhost:5432` | Connection URL |

### URL Generation

URLs are generated based on the Docker image name:

- `postgres:*` -> `postgres://user:pass@localhost:port` (credentials from env)
- `redis:*` -> `redis://localhost:port`
- Multi-port services -> `localhost:port` (no protocol)
- All others -> `http://localhost:port`

### Named Ports

For infra with `[infra.<name>.ports]`, additional variables are created:

| Variable | Example |
|---|---|
| `DEVRIG_<NAME>_PORT_<PORTNAME>` | `DEVRIG_MAILPIT_PORT_SMTP=1025` |

## Service-to-Service Variables

Each service sees variables for every *other* service:

| Variable | Example |
|---|---|
| `DEVRIG_<NAME>_HOST` | `DEVRIG_API_HOST=localhost` |
| `DEVRIG_<NAME>_PORT` | `DEVRIG_API_PORT=3000` |
| `DEVRIG_<NAME>_URL` | `DEVRIG_API_URL=http://localhost:3000` |

A service does NOT see its own `DEVRIG_*` vars. Instead it gets `PORT` and `HOST`.

## Template Expressions

Service env values support `{{ dotted.path }}` template expressions:

```toml
[services.api.env]
DATABASE_URL = "postgres://devrig:devrig@localhost:{{ infra.postgres.port }}/myapp"
SMTP_PORT = "{{ infra.mailpit.ports.smtp }}"
```

Available template variables:
- `project.name`
- `services.<name>.port`
- `infra.<name>.port`
- `infra.<name>.ports.<portname>`
- `compose.<name>.port`

## Inspecting Variables

Use `devrig env <service>` to see the full resolved environment for a service:

```bash
devrig env api
```
