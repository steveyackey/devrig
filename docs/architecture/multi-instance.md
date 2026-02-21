# Multi-Instance Isolation

## Overview

devrig supports running multiple projects simultaneously on the same machine.
Each project is identified by a unique slug, and state is scoped to prevent
interference between instances.

## Project identity derivation

A project's identity is computed in `identity.rs` using two inputs:

1. **Project name** -- The `name` field from the `[project]` section of
   `devrig.toml`.
2. **Config path hash** -- The SHA-256 hash of the **canonical absolute path**
   to the config file, truncated to the first 8 hex characters (4 bytes).

The canonical path is obtained by calling `std::path::Path::canonicalize()`,
which resolves symlinks, removes `.` and `..` components, and produces an
absolute path. This ensures that different ways of referencing the same file
(relative paths, symlinks, trailing slashes) produce the same hash.

### Slug format

```
{name}-{hash}
```

Example: if the project name is `myapp` and the config is at
`/home/user/projects/myapp/devrig.toml`, the slug might be `myapp-a1b2c3d4`.

The hash provides collision resistance. Two projects named `myapp` in
different directories get different slugs. The same project always gets the
same slug as long as it stays at the same path.

## State scoping

### Per-project state: `.devrig/`

Each project stores runtime state in a `.devrig/` directory adjacent to its
`devrig.toml`:

```
myproject/
  devrig.toml
  .devrig/
    state.json      # Running services, PIDs, ports, start time
    kubeconfig      # (future) k3d cluster credentials
```

The `state.json` file contains:

```json
{
  "slug": "myapp-a1b2c3d4",
  "config_path": "/home/user/projects/myapp/devrig.toml",
  "services": {
    "api": { "pid": 12345, "port": 3000, "port_auto": false },
    "web": { "pid": 12346, "port": 5173, "port_auto": false }
  },
  "started_at": "2025-01-15T10:30:00Z"
}
```

State is written atomically (write to `.json.tmp`, then rename) to avoid
corruption if the process is killed mid-write.

`devrig stop` removes `state.json`. `devrig delete` removes the entire
`.devrig/` directory.

### Global registry: `~/.devrig/instances.json`

A machine-wide registry at `~/.devrig/instances.json` tracks all active devrig
instances. Each entry contains:

- `slug` -- The project slug.
- `config_path` -- Absolute path to the config file.
- `state_dir` -- Absolute path to the `.devrig/` directory.
- `started_at` -- ISO 8601 timestamp.

This registry powers `devrig ps --all`, which lists all running instances
across the machine.

The registry is automatically cleaned up: entries whose `state.json` no longer
exists on disk are removed when the registry is read (via `cleanup()`).

The registry file is also written atomically via temp file + rename.

## Port collision detection

At startup, devrig checks every fixed port in the configuration by attempting
to bind to it with `TcpListener::bind(("127.0.0.1", port))`. If the bind
fails, the port is already in use.

On Linux, devrig additionally identifies the process holding the port by
parsing `/proc/net/tcp` to find the socket inode, then scanning `/proc/*/fd/`
to match the inode to a PID and reading `/proc/{pid}/cmdline`.

This catches conflicts between:

- Two devrig instances that declare the same fixed port.
- A devrig service and any other software on the machine.

Auto-assigned ports (`port = "auto"`) use ephemeral OS ports obtained by
binding to port 0, which the OS guarantees are not in use at assignment time.
