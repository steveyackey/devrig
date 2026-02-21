# Architecture Overview

## System architecture

devrig is a single Rust binary that orchestrates local development services.
It follows a layered architecture where the CLI dispatches to an orchestrator,
which coordinates supervisors, log multiplexing, and state management.

```
 +------------------+
 |    CLI (clap)    |   Parses args, dispatches to commands or orchestrator
 +--------+---------+
          |
          v
 +------------------+
 |   Config Layer   |   Reads devrig.toml, deserializes, validates
 |  (toml + serde)  |
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
 |   LogWriter (mpsc chan)   |   Multiplexes stdout/stderr from all services
 +---------------------------+
```

## Runtime model

devrig uses the Tokio async runtime with a multi-threaded executor. The flow
for `devrig start` is:

1. **Config resolution** -- Walk up the directory tree to find `devrig.toml`
   (or use the path from `-f`). Parse the TOML into `DevrigConfig`. Run
   semantic validation (dependency references, port conflicts, cycle
   detection).

2. **Identity computation** -- Canonicalize the config file path and compute
   a SHA-256 hash (truncated to 8 hex chars). Combine with the project name
   to form the slug (e.g. `myapp-a1b2c3d4`).

3. **Dependency resolution** -- Build a `petgraph::DiGraph` where edges point
   from dependency to dependent. Topologically sort to get the startup order.

4. **Port resolution** -- Check fixed ports for availability (bind test).
   Assign ephemeral OS ports for `port = "auto"` entries.

5. **Service supervision** -- For each service in dependency order, spawn a
   `ServiceSupervisor` as a Tokio task. Each supervisor:
   - Runs the command via `sh -c` in a new process group.
   - Pipes stdout/stderr through async readers into a shared `mpsc` channel.
   - On exit, applies exponential backoff and restarts (up to `max_restarts`).
   - Responds to cancellation via `CancellationToken`.

6. **Log multiplexing** -- A `LogWriter` task reads from the shared channel
   and prints each line with a color-coded service prefix.

7. **State persistence** -- Project state (slug, service list, ports, start
   time) is saved to `.devrig/state.json`. The instance is registered in the
   global registry at `~/.devrig/instances.json`.

8. **Shutdown** -- On Ctrl+C (or when all services exit), the orchestrator
   cancels all supervisors. Each supervisor sends SIGTERM to the process group,
   waits 5 seconds, then escalates to SIGKILL. State files are cleaned up.

## Component responsibilities

| Component           | Module                    | Role                                        |
|---------------------|---------------------------|---------------------------------------------|
| CLI                 | `cli.rs`                  | Argument parsing via clap derive macros      |
| Config model        | `config/model.rs`         | Data structures, custom Port deserializer    |
| Config resolution   | `config/resolve.rs`       | Walk-up file discovery, -f flag handling     |
| Config validation   | `config/validate.rs`      | Semantic checks (deps, ports, cycles)        |
| Project identity    | `identity.rs`             | Slug computation from name + path hash       |
| Orchestrator        | `orchestrator/mod.rs`     | Top-level coordination for start/stop/delete |
| Dependency graph    | `orchestrator/graph.rs`   | petgraph DAG, topological sort               |
| Port management     | `orchestrator/ports.rs`   | Availability checks, auto-assignment         |
| Instance registry   | `orchestrator/registry.rs`| Global ~/.devrig/instances.json tracking     |
| Project state       | `orchestrator/state.rs`   | Per-project .devrig/state.json persistence   |
| Service supervisor  | `orchestrator/supervisor.rs` | Process lifecycle, restart, signal handling|
| Log writer          | `ui/logs.rs`              | Multiplexed, color-coded log output          |
| Startup summary     | `ui/summary.rs`           | Table showing services, ports, status        |
| Init command        | `commands/init.rs`        | Scaffold devrig.toml with project detection  |
| Doctor command      | `commands/doctor.rs`      | Check external tool availability             |
| Ps command          | `commands/ps.rs`          | Display service status (local and global)    |

## Tech stack

| Concern              | Crate / Tool               |
|----------------------|----------------------------|
| Async runtime        | tokio (multi-thread)       |
| CLI parsing          | clap (derive)              |
| Config parsing       | toml + serde               |
| Dependency graphs    | petgraph                   |
| Hashing              | sha2 + hex                 |
| Process signals      | nix                        |
| Error handling       | anyhow + thiserror + miette|
| Colored output       | owo-colors                 |
| Tracing              | tracing + tracing-subscriber|
| Task management      | tokio-util (TaskTracker, CancellationToken) |
| Time                 | chrono                     |
