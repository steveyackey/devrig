# Getting Started

This guide walks you through installing devrig, initializing a project, and
running your first set of services.

## Prerequisites

- **Docker** — required for container-based services and the local cluster.

The cluster tools — **k3d**, **kubectl**, and **helm** — do **not** need to be
pre-installed. devrig fetches pinned, checksum-verified copies on demand into
`~/.devrig/bin` (run `devrig deps list` to see them, or `devrig deps install`
to fetch ahead of time). If you'd rather use your own copies on `PATH`, set
`[tools] prefer = "system"`. `devrig doctor` reports which copy of each tool
will be used.

## Install

### Prebuilt binary (recommended)

Download the archive for your platform from the
[latest release](https://github.com/steveyackey/devrig/releases/latest),
extract it, and put `devrig` on your `PATH`. The release binaries ship with the
embedded web dashboard.

Once installed, `devrig update` self-updates the binary from the latest GitHub
release.

> **Upgrading from the Rust version (≤ 0.30.x)?** `devrig update` on a Rust build
> won't pull the Go release — the Rust binary delegates to the cargo-dist
> `devrig-update` sidecar, which only recognizes cargo-dist release assets, while
> Go releases are built by goreleaser. Re-run the installer
> ([`install.sh`](https://github.com/steveyackey/devrig/releases/latest/download/install.sh)
> /
> [`install.ps1`](https://github.com/steveyackey/devrig/releases/latest/download/install.ps1))
> once to switch to the Go build. After that, `devrig update` self-updates
> normally and cleans up the leftover Rust install receipt and `devrig-update`
> sidecar on its first run.

### go install (no embedded dashboard)

```bash
go install github.com/steveyackey/devrig/cmd/devrig@latest
```

`go install` builds the CLI without the embedded dashboard assets (the dashboard
served on the OTLP/dashboard port). If you need the dashboard, use a release
binary or build from source (see below). Everything else works the same.

### From source

```bash
git clone https://github.com/steveyackey/devrig
cd devrig/web && pnpm install && pnpm run build-only
cd .. && cp -r web/dist/* internal/dashboard/dist/
go build -tags embedspa ./cmd/devrig
```

This produces a `devrig` binary in the repo root with the dashboard embedded.

Verify the installation:

```bash
devrig --version
```

## Check dependencies

Run the doctor command to verify that external tools are available:

```bash
devrig doctor
```

You will see output like:

```
devrig doctor
=============

  [ok] docker           Docker version 24.0.7
  [ok] k3d              k3d version v5.6.0
  [ok] kubectl          Client Version: v1.28.4
  [ok] helm             v3.16.2

All dependencies found.
```

Missing tools are shown with `[!!]`. Docker and k3d are only needed for
container and cluster features (not required for basic service orchestration).

## Initialize a project

Navigate to your project directory and run:

```bash
cd ~/my-project
devrig init
```

This creates a `devrig.toml` tailored to your project type. devrig detects
whether the directory contains `go.mod`, `package.json`, `Cargo.toml`, or
Python files and generates an appropriate starter command.

Example output:

```
Created devrig.toml in /home/user/my-project

  Project: my-project
  Service: server -> go run ./...

Edit the file, then run `devrig start` to begin.
```

## Edit the configuration

Open `devrig.toml` and adjust it for your project. A typical configuration
for a project with an API and a frontend:

```toml
[project]
name = "my-project"

[services.api]
command = "go run ./cmd/api"
port = 3000
path = "./api"

[services.web]
command = "npm run dev"
port = 5173
path = "./web"
depends_on = ["api"]
```

See [configuration.md](configuration.md) for the full reference.

## Start services

```bash
devrig start
```

devrig starts services in dependency order (in this example, `api` before
`web`) and prints a startup summary:

```
  devrig  my-project (a1b2c3)

  Services:

    api              http://localhost:3000            running
    web              http://localhost:5173            running

  Press Ctrl+C to stop all services
```

Service stdout/stderr is captured to a JSONL log file and the in-memory OTel
store rather than printed to the terminal. Use `devrig logs` to query it, or
open the dashboard.

You can also start specific services (and their dependencies):

```bash
devrig start web
```

This starts both `api` (as a dependency) and `web`.

## Check status

In another terminal, check what is running:

```bash
devrig ps
```

To see all devrig instances across the machine:

```bash
devrig ps --all
```

## Stop services

Press Ctrl+C in the terminal running `devrig start`, or from another terminal:

```bash
devrig stop
```

This sends SIGTERM to all service process groups, waits up to 10 seconds for
graceful shutdown, then cleans up state.

Stopping always affects the whole project — all services run under one
supervisor process, so there is no per-service stop. To restart a subset,
run `devrig stop` and then `devrig start <service>`.

## Delete state

To stop services and remove the `.devrig/` state directory:

```bash
devrig delete
```

## Using a different config file

To use a config file other than `devrig.toml` (for example, a staging
configuration):

```bash
devrig start -f devrig.staging.toml
```

The `-f` flag works with all commands:

```bash
devrig ps -f devrig.staging.toml
devrig stop -f devrig.staging.toml
```

## Next steps

- Read the [Configuration Guide](configuration.md) for the full TOML reference.
- Run `devrig doctor` to check for optional dependencies.
- See [Architecture Overview](../architecture/overview.md) to understand how
  devrig works internally.
