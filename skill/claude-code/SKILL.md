---
name: devrig
description: Manage and debug a devrig development environment. Use when the user asks about service health, errors, performance, traces, logs, metrics, debugging, project setup, configuration, starting/stopping services, or anything related to their local dev environment managed by devrig.
allowed-tools:
  - Bash(devrig *)
  - Read(devrig.toml)
  - Edit(devrig.toml)
---

# devrig — Local Development Environment

You have access to devrig, a local development orchestrator with built-in OpenTelemetry collection.

## Getting Help

Always use the CLI for up-to-date information about commands and options:

```bash
devrig --help                    # All commands
devrig <command> --help          # Options for a specific command
devrig skill reference           # Full configuration field reference
```

## Key Concepts

### Configuration

Config file: `devrig.toml` (found by walking up from cwd). Override with `-f <path>`.

Run `devrig skill reference` to see the complete field reference for all config sections.

### Template Expressions

`{{ dotted.path }}` templates work in `[env]`, `[services.*.env]`, and addon `values`:

- `{{ docker.<name>.port }}` — resolved docker port
- `{{ docker.<name>.ports.<portname> }}` — named port (alias: `{{ docker.<name>.port_<portname> }}`)
- `{{ cluster.image.<name>.tag }}` — built cluster image tag
- `{{ cluster.kubeconfig }}` — path to the k3d cluster kubeconfig
- `{{ project.name }}`, `{{ dashboard.port }}`, `{{ dashboard.otel.grpc_port }}`, etc.

Unresolved variables produce an error with "did you mean?" suggestions.

### Environment Variable Expansion

- `$VAR` / `${VAR}` expands from `.env` files or host environment
- `$$` for a literal `$`
- Expansion runs before template interpolation

### Auto-injected Variables

Every service automatically receives:

- `DEVRIG_<NAME>_HOST`, `DEVRIG_<NAME>_PORT`, `DEVRIG_<NAME>_URL` for all other services/docker containers
- `DEVRIG_<NAME>_PORT_<PORTNAME>` for named ports

When dashboard is enabled, every service also gets:

- `OTEL_EXPORTER_OTLP_ENDPOINT` — OTLP gRPC endpoint
- `OTEL_SERVICE_NAME` — service name from config
- `DEVRIG_DASHBOARD_URL` — dashboard URL

## Common Workflows

### Setting Up a New Project

```bash
devrig init          # Generate starter devrig.toml
devrig validate      # Check config for errors
devrig start         # Launch everything
```

### Debugging Performance Issues

```bash
devrig query traces --min-duration 500 --limit 10   # Find slow traces
devrig query trace <trace-id>                        # Inspect a trace
devrig query related <trace-id>                      # Logs + metrics for a trace
```

### Investigating Errors

```bash
devrig query traces --status error --limit 10        # Find error traces
devrig query logs --level error --limit 30           # Search error logs
devrig query logs --service <name> --search "timeout" # Narrow to a service
```

### Checking System Health

```bash
devrig ps                                            # Service status and ports
devrig query status                                  # OTel collector summary
devrig query metrics --limit 50                      # Recent metrics
```

### Cluster Addons

Helm addons support remote charts (with `repo`), local charts (path), and OCI charts (`oci://` URL):

```toml
# OCI chart — no repo field needed
[cluster.addons.my-chart]
type = "helm"
chart = "oci://ghcr.io/org/charts/my-chart"
namespace = "my-chart"
version = "1.2.0"
```

## Tips

- Use `devrig env <service>` to see exactly what env vars a service receives
- Use `jq` for filtering: `devrig query traces --format jsonl | jq 'select(.has_error)'`
- Output formats: `--format table` (human), `--format json` (pretty), `--format jsonl` (pipe to jq)
- `devrig logs -F` for live tailing, `devrig query logs` for OTel-collected logs
