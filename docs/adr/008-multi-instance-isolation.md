# ADR 008: Multi-instance isolation via project slug

## Status

Accepted

## Context

Developers often work on multiple projects simultaneously, each with its own
`devrig.toml`. These projects may run at the same time on the same machine.
We need to ensure:

- State from one project does not interfere with another.
- Port assignments do not collide silently.
- Resources (containers, clusters) are scoped to the correct project.
- The `devrig ps --all` command can enumerate all running instances.

## Decision

Each project is identified by a **slug** derived from two components:

- The `name` field from `[project]` in `devrig.toml`.
- A SHA-256 hash of the **canonical absolute path** to the config file,
  truncated to 8 hex characters.

The slug format is `{name}-{hash}`, for example `myapp-a1b2c3d4`.

State is scoped as follows:

- **Per-project state** is stored in `.devrig/state.json` next to the config
  file. This includes the list of running services, their PIDs, and assigned
  ports.
- **Global registry** at `~/.devrig/instances.json` tracks all active devrig
  instances across the machine. Each entry records the slug, config path,
  state directory, and start time.

The path is canonicalized before hashing so that different representations
of the same path (relative, with symlinks, trailing slashes) produce the
same slug.

## Consequences

**Positive:**

- Deterministic: the same project at the same path always gets the same slug.
  Renaming the directory changes the slug, which is the correct behavior since
  it is effectively a different project location.
- No collisions: two projects with the same name but different paths get
  different hashes. Two projects at the same path always have the same hash.
- Clean isolation: each project's `.devrig/` directory is independent.
- Port collision detection checks all fixed ports against the OS at startup,
  catching conflicts between devrig instances or with other software.
- `devrig ps --all` reads the global registry to show all instances, with
  automatic cleanup of stale entries.

**Negative:**

- Moving a project directory changes its slug, which means `devrig stop` in
  the new location will not find the state from the old location. This is
  an edge case and is mitigated by `devrig delete` which cleans up state
  at the current location.
- The global registry at `~/.devrig/instances.json` is a shared mutable file.
  Concurrent writes could race, though this is unlikely in practice since
  instance registration happens only at startup. Atomic writes (write to
  temp file, then rename) mitigate this.
