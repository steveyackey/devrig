# Contributing

## Development setup

### Prerequisites

- Go (see `go.mod` for the required version)
- Node.js 20+ and [pnpm](https://pnpm.io/) (for the `web/` dashboard and `e2e/`)
- Git

### Clone and build

```bash
git clone https://github.com/steveyackey/devrig.git
cd devrig
go build ./...
```

To build the binary **with the embedded dashboard**:

```bash
cd web && pnpm install && pnpm run build-only
cd .. && cp -r web/dist/* internal/dashboard/dist/
go build -tags embedspa ./cmd/devrig
```

### Run from source

```bash
go run ./cmd/devrig start
go run ./cmd/devrig doctor
go run ./cmd/devrig --help
```

For dashboard hot-reload, run with `--dev` (spawns Vite on `:5173`, proxying
`/api` and `/ws` to the Go server on `:4000`):

```bash
go run ./cmd/devrig start --dev -f devrig.run.toml
```

### Run tests

```bash
go test ./...
go vet ./...
```

Some tests spawn real processes and bind ports; they are slower than pure unit
tests and should pass before submitting a PR.

### Frontend checks

```bash
cd web
pnpm run check        # format + lint (Biome) + checks
pnpm run type-check   # vue-tsc
```

### E2E tests

The dashboard E2E suite uses Playwright via vitest (`vp test`) and requires a
running devrig with the embedded dashboard:

```bash
cd e2e && pnpm test
```

## Code organization

```
cmd/devrig/
  main.go                  Entrypoint. Wires up the cobra command tree and
                           dispatches to commands or the orchestrator.

internal/
  identity/identity.go     ProjectIdentity: name + SHA-256 slug derivation.

  config/
    load.go                Reads the config file and decodes TOML/YAML.
    model.go               Config, ServiceConfig, Port with custom
                           UnmarshalTOML/UnmarshalYAML.
    resolve.go             Walk-up config discovery and -f flag handling.
    interpolate.go         Template-variable interpolation.
    validate.go            Semantic validation: dependency refs, duplicate
                           ports, cycles, empty commands.

  graph/graph.go           Resolver: unified dependency DAG (services, docker,
                           compose, cluster) with Kahn's topological sort.

  orchestrator/
    orchestrator.go        Orchestrator: coordinates start, stop, delete flows.
                           Spawns supervisors, manages state.
    signal_unix.go /       Per-platform shutdown signal handling (build tags).
    signal_windows.go

  supervisor/
    supervisor.go          Process lifecycle, stdout/stderr piping, restart
                           with exponential backoff, SIGTERM/SIGKILL shutdown.
    process_unix.go /      Per-platform process-group control (build tags).
    process_windows.go

  ports/ports.go           Availability checks, free-port assignment, port
                           owner identification.
  registry/registry.go     Global ~/.devrig/instances.json registry.
  state/state.go           Per-project .devrig/state.json persistence.

  otel/                    In-memory OTLP receiver + telemetry store.
  dashboard/               net/http API, WebSocket, embedded SPA (go:embed).
  docker/, compose/,       Docker, compose, and k3d cluster integrations.
  cluster/

  commands/
    init.go                Generate starter devrig.toml. Detects project type
                           (go.mod, package.json, Cargo.toml, Python).
    doctor.go              Checks for docker, k3d, kubectl, helm.
    ps.go                  Displays local project status or all instances.
    start.go, stop.go,     One file per subcommand.
    logs.go, query.go, ...

web/                       Vue 3 + Tailwind v4 + Vite dashboard (pnpm).
e2e/                       Playwright + vitest dashboard tests (pnpm).
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
   go build ./... && go vet ./... && go test ./...
   ```
   For dashboard changes, also run `cd web && pnpm run check && pnpm run type-check`.

4. **Follow existing patterns.** The codebase wraps errors with `fmt.Errorf`
   and `%w`, threads a `context.Context` for cancellation, and sorts map keys
   explicitly where deterministic ordering matters.

5. **Update documentation** if your change affects user-facing behavior,
   configuration options, or architectural decisions.

6. **Commit messages** should be concise and describe the "why" rather than
   the "what." Use conventional prefixes when appropriate: `feat:`, `fix:`,
   `refactor:`, `test:`, `docs:`.

7. **ADRs for design changes.** If your PR changes a significant design
   decision, add a new ADR in `docs/adr/` following the existing numbered
   format.
