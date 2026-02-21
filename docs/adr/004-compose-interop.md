# ADR 004: Docker Compose interoperability via delegation

## Status

Accepted

## Context

Many existing projects already have a `docker-compose.yml` that defines
infrastructure services (databases, message queues, caches). These files
represent significant investment in configuration and are well-understood by
the team.

devrig could either:

1. Re-implement all Docker Compose functionality natively.
2. Ignore existing Compose files and require users to rewrite everything in
   devrig.toml.
3. Delegate to Docker Compose for services that already have Compose
   definitions.

Re-implementing Compose is a large undertaking and would constantly lag behind
upstream features. Ignoring existing Compose files creates a high adoption
barrier.

## Decision

Support delegating to existing Docker Compose files by allowing a service
command to simply invoke `docker compose`:

```toml
[services.infra]
command = "docker compose up"
path = "./infra"
```

devrig treats this like any other service command. It starts the process,
captures its stdout/stderr, and manages its lifecycle. The Compose file remains
the source of truth for container definitions.

In future versions, devrig may add first-class `[infra.*]` blocks for native
container management, but Compose delegation will remain supported as an escape
hatch.

## Consequences

**Positive:**

- Gradual adoption: teams can start using devrig without rewriting their
  Compose setup.
- Does not reinvent the wheel. Compose handles volumes, networks, health
  checks, and image pulling.
- Users keep the flexibility to use any Compose features, including profiles
  and extensions.

**Negative:**

- devrig has limited visibility into individual containers managed by Compose.
  Port discovery and health checks operate at the Compose process level, not
  per-container.
- Log multiplexing shows Compose's own output (which includes its own service
  prefixes), leading to double-prefixed lines.

**Mitigations:**

- Future `[infra.*]` blocks will provide native container management with
  tighter integration for projects that want it.
- Users can pass `--no-log-prefix` to `docker compose up` to avoid double
  prefixing.
