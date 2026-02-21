# Contributing

## Development setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Git

### Clone and build

```bash
git clone https://github.com/your-org/devrig.git
cd devrig
cargo build
```

### Run from source

```bash
cargo run -- start
cargo run -- doctor
cargo run -- --help
```

### Run tests

```bash
cargo test
```

### Run integration tests

Integration tests are gated behind a feature flag to avoid running them in
quick feedback loops:

```bash
cargo test --features integration
```

Integration tests may spawn real processes and bind ports. They are slower
than unit tests and should be run before submitting a PR.

### Check formatting and lints

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## Code organization

```
src/
  main.rs                  Entrypoint. Initializes tracing, parses CLI,
                           dispatches to commands or orchestrator.

  lib.rs                   Declares public modules.

  cli.rs                   Clap derive structs (Cli, GlobalOpts, Commands).

  identity.rs              ProjectIdentity: name + SHA-256 slug derivation.

  config/
    mod.rs                 load_config() -- reads file and deserializes TOML.
    model.rs               DevrigConfig, ServiceConfig, Port enum with
                           custom serde Visitor.
    resolve.rs             find_config() walk-up search, resolve_config()
                           for -f flag.
    validate.rs            Semantic validation: dependency refs, duplicate
                           ports, cycles, empty commands.

  orchestrator/
    mod.rs                 Orchestrator struct. Coordinates start, stop,
                           delete flows. Spawns supervisors, manages state.
    graph.rs               DependencyResolver using petgraph DiGraph.
                           Topological sort for start order.
    ports.rs               check_port_available(), find_free_port(),
                           identify_port_owner() (Linux /proc parsing).
    registry.rs            InstanceRegistry: global ~/.devrig/instances.json.
    state.rs               ProjectState: per-project .devrig/state.json.
    supervisor.rs          ServiceSupervisor: process lifecycle, stdout/stderr
                           piping, restart with exponential backoff, SIGTERM/
                           SIGKILL shutdown.

  commands/
    mod.rs                 Module declarations for subcommands.
    init.rs                Generate starter devrig.toml. Detects project type
                           (Cargo.toml, package.json, go.mod, Python).
    doctor.rs              Checks for docker, k3d, kubectl, cargo-watch.
    ps.rs                  Displays local project status or all instances.

  ui/
    mod.rs                 Module declarations for UI components.
    logs.rs                LogWriter: async mpsc receiver, color-coded output.
    summary.rs             print_startup_summary(): table of services, ports.
```

## Architecture decisions

Significant design decisions are recorded in [docs/adr/](../adr/). Read these
before proposing changes to core behavior:

- [001 - TOML only](../adr/001-toml-only.md)
- [002 - No profiles](../adr/002-no-profiles.md)
- [003 - Isolated kubeconfig](../adr/003-isolated-kubeconfig.md)
- [004 - Compose interop](../adr/004-compose-interop.md)
- [005 - Traefik over Nginx](../adr/005-traefik-over-nginx.md)
- [006 - In-memory OTel](../adr/006-in-memory-otel.md)
- [007 - Agent browser testing](../adr/007-agent-browser-testing.md)
- [008 - Multi-instance isolation](../adr/008-multi-instance-isolation.md)

## PR guidelines

1. **One concern per PR.** Keep PRs focused on a single feature, bug fix, or
   refactor.

2. **Write tests.** New functionality should include unit tests. Changes to
   the orchestrator or supervisor should include integration tests where
   feasible.

3. **Run the full check suite** before submitting:
   ```bash
   cargo fmt --check && cargo clippy -- -D warnings && cargo test
   ```

4. **Follow existing patterns.** The codebase uses `anyhow::Result` for
   fallible operations, `thiserror` for typed error enums in library code,
   and `BTreeMap` (not `HashMap`) for deterministic ordering.

5. **Update documentation** if your change affects user-facing behavior,
   configuration options, or architectural decisions.

6. **Commit messages** should be concise and describe the "why" rather than
   the "what." Use conventional prefixes when appropriate: `feat:`, `fix:`,
   `refactor:`, `test:`, `docs:`.

7. **ADRs for design changes.** If your PR changes a significant design
   decision, add a new ADR in `docs/adr/` following the existing numbered
   format.
