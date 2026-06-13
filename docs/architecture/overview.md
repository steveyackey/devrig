# Architecture Overview

## System architecture

devrig is a single Go binary that orchestrates local development services.
It follows a layered architecture where the CLI dispatches to an orchestrator,
which coordinates supervisors, log multiplexing, and state management.

```
 +------------------+
 |   CLI (cobra)    |   Parses args, dispatches to commands or orchestrator
 +--------+---------+
          |
          v
 +------------------+
 |   Config Layer   |   Reads devrig.toml, decodes, validates
 |  (BurntSushi/toml)|
 +--------+---------+
          |
          v
 +------------------+
 |   Orchestrator   |   Coordinates startup, shutdown, state management
 +--------+---------+
          |
    +-----+-----+-----+-----+
    |     |     |     |     |
    v     v     v     v     v
 +-----+ +-----+ +-----+ +-------+ +----------+
 |Sup 1| |Sup 2| |Sup N| | Ports | | Registry |
 +--+--+ +--+--+ +--+--+ +-------+ +----------+
    |        |       |
    v        v       v
 +---------------------------+
 |  Log bridge (goroutines)  |   Multiplexes stdout/stderr from all services
 +---------------------------+
```

## Runtime model

devrig uses Go goroutines and channels for concurrency, with a `context.Context`
threaded through for cancellation. The flow for `devrig start` is:

1. **Config resolution** -- Walk up the directory tree to find `devrig.toml`
   (or use the path from `-f`). Decode the TOML into the `Config` struct. Run
   semantic validation (dependency references, port conflicts, cycle
   detection).

2. **Identity computation** -- Canonicalize the config file path and compute
   a SHA-256 hash (truncated to 6 hex chars). Combine with the project name
   to form the slug (e.g. `myapp-a1b2c3`).

3. **Dependency resolution** -- Build a unified dependency graph (services,
   docker containers, compose services, cluster resources) and run Kahn's
   topological sort to get the startup order.

4. **Port resolution** -- Check fixed ports for availability (bind test).
   Assign ephemeral OS ports for `port = "auto"` entries.

5. **Service supervision** -- For each service in dependency order, spawn a
   supervisor goroutine. Each supervisor:
   - Runs the command via `sh -c` (or `cmd /C` on Windows) in a new process group.
   - Pipes stdout/stderr through readers into the log bridge.
   - On exit, applies exponential backoff and restarts (up to `max_restarts`).
   - Responds to cancellation via the shared `context.Context`.

6. **Log multiplexing** -- Service stdout/stderr is bridged to a JSONL log file
   and to the in-memory OTel store (consumed by the dashboard and `devrig logs`),
   rather than printed to the terminal.

7. **State persistence** -- Project state (slug, service list, ports, start
   time) is saved to `.devrig/state.json`. The instance is registered in the
   global registry at `~/.devrig/instances.json`.

8. **Shutdown** -- On Ctrl+C (or when all services exit), the orchestrator
   cancels the context. Each supervisor sends SIGTERM to the process group,
   waits, then escalates to SIGKILL. State files are cleaned up.

## Component responsibilities

| Component           | Module                       | Role                                        |
|---------------------|------------------------------|---------------------------------------------|
| Entrypoint / CLI    | `cmd/devrig/main.go`         | Argument parsing via cobra, command wiring   |
| Config model        | `internal/config/model.go`   | Data structures, custom Port unmarshaler     |
| Config resolution   | `internal/config/resolve.go` | Walk-up file discovery, -f flag handling     |
| Config validation   | `internal/config/validate.go`| Semantic checks (deps, ports, cycles)        |
| Project identity    | `internal/identity/identity.go` | Slug computation from name + path hash    |
| Orchestrator        | `internal/orchestrator/orchestrator.go` | Top-level coordination for start/stop/delete |
| Dependency graph    | `internal/graph/graph.go`    | Unified DAG, Kahn's topological sort         |
| Port management     | `internal/ports/ports.go`    | Availability checks, auto-assignment         |
| Instance registry   | `internal/registry/registry.go` | Global ~/.devrig/instances.json tracking  |
| Project state       | `internal/state/state.go`    | Per-project .devrig/state.json persistence   |
| Service supervisor  | `internal/supervisor/supervisor.go` | Process lifecycle, restart, signal handling|
| Init command        | `internal/commands/init.go`  | Scaffold devrig.toml with project detection  |
| Doctor command      | `internal/commands/doctor.go`| Check external tool availability             |
| Ps command          | `internal/commands/ps.go`    | Display service status (local and global)    |

## Tech stack

| Concern              | Library / Tool             |
|----------------------|----------------------------|
| Concurrency          | goroutines + channels      |
| CLI parsing          | spf13/cobra                |
| Config parsing       | BurntSushi/toml (+ yaml.v3)|
| Dependency graphs    | custom Kahn topological sort |
| Hashing              | crypto/sha256 (stdlib)     |
| Process signals      | os/exec + os.Signal (stdlib) |
| Frontend embedding   | go:embed                   |
| Cancellation         | context.Context            |
| Time                 | time (stdlib)              |
