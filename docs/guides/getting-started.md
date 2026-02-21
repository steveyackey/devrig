# Getting Started

This guide walks you through installing devrig, initializing a project, and
running your first set of services.

## Prerequisites

- **Rust toolchain** (1.75+) with `cargo`
- **Git** for cloning the repository

Optional (checked by `devrig doctor`):

- **Docker** for container-based services
- **k3d** for local Kubernetes clusters
- **kubectl** for cluster interaction
- **cargo-watch** for Rust hot reload

## Install from source

```bash
git clone https://github.com/your-org/devrig.git
cd devrig
cargo install --path .
```

This installs the `devrig` binary into `~/.cargo/bin/`. Make sure this
directory is in your `PATH`.

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
  [ok] cargo-watch      cargo-watch 8.4.1

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
whether the directory contains `Cargo.toml`, `package.json`, `go.mod`, or
Python files and generates an appropriate starter command.

Example output:

```
Created devrig.toml in /home/user/my-project

  Project: my-project
  Service: app -> cargo watch -x run

Edit the file, then run `devrig start` to begin.
```

## Edit the configuration

Open `devrig.toml` and adjust it for your project. A typical configuration
for a project with an API and a frontend:

```toml
[project]
name = "my-project"

[services.api]
command = "cargo watch -x run"
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
`web`) and multiplexes their logs with color-coded prefixes:

```
  devrig  my-project (a1b2c3d4)

  Services:

    api              http://localhost:3000            running
    web              http://localhost:5173            running

  Press Ctrl+C to stop all services

api | Compiling my-project v0.1.0
api | Listening on 0.0.0.0:3000
web | VITE v5.0.0  ready in 200 ms
web | Local: http://localhost:5173/
```

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
