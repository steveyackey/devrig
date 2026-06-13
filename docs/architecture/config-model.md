# Configuration Model

## TOML schema

devrig uses a single `devrig.toml` file with the following top-level structure:

```toml
[project]
name = "myapp"          # Required. Used in slug, container names, etc.

[env]                   # Optional. Global env vars inherited by all services.
DATABASE_URL = "..."

[services.<name>]       # Zero or more service definitions.
command = "..."         # Required. The shell command to run.
path = "./"             # Optional. Working directory relative to config file.
port = 3000             # Optional. Fixed port number or "auto".
depends_on = ["other"]  # Optional. List of service names this depends on.

[services.<name>.env]   # Optional. Per-service env vars (override globals).
API_KEY = "secret"
```

## Data model

The TOML is decoded into these Go types (defined in `internal/config/model.go`):

```go
type Config struct {
    Project  ProjectConfig            `toml:"project"`
    Services map[string]ServiceConfig `toml:"services"`
    Env      map[string]string        `toml:"env"`
    // ...docker, compose, cluster, dashboard, oidc, network, links
}

type ProjectConfig struct {
    Name string `toml:"name"`
}

type ServiceConfig struct {
    Path      *string           `toml:"path"`
    Command   string            `toml:"command"`
    Port      *Port             `toml:"port"`
    Env       map[string]string `toml:"env"`
    DependsOn []string          `toml:"depends_on"`
}

// Port is either a fixed port number (Value > 0) or "auto" (Auto == true).
type Port struct {
    Value uint16
    Auto  bool
}
```

## Two-phase parsing

Configuration loading happens in two distinct phases:

### Phase 1: Decoding

The `BurntSushi/toml` decoder reads the file into the `Config` struct (a
`yaml.v3` decoder handles `devrig.yaml`). Structural errors (wrong types,
malformed TOML) are caught here. Devrig also wires up custom unmarshalers for
nested types such as `Port` and `StringOrList`.

### Phase 2: Semantic validation

The `Validate()` function in `internal/config/validate.go` performs cross-field
checks that decoding alone cannot express:

1. **Dependency existence** -- Every entry in `depends_on` must reference a
   service name that exists in `[services.*]`.
2. **Duplicate port detection** -- No two services may declare the same fixed
   port number.
3. **Cycle detection** -- The dependency graph must be a DAG. Cycles are
   detected via iterative DFS with a visited/in-stack approach.
4. **Empty command check** -- The `command` field must not be blank or
   whitespace-only.

All errors are collected and reported together, rather than failing on the
first error. This lets users fix multiple issues in a single edit cycle.

## Port type design

The `Port` type supports two representations in TOML:

```toml
port = 3000     # Fixed port: Port{Value: 3000}
port = "auto"   # Auto-assign: Port{Auto: true}
                 # (omitted): *Port is nil
```

This is implemented via custom `UnmarshalTOML`/`UnmarshalYAML` methods that
accept both an integer (the fixed port number) and the `"auto"` string.

Range validation happens inside the unmarshaler: integers outside 1-65535
produce a descriptive error. Strings other than `"auto"` are rejected.

Helper methods on `Port`:
- `AsFixed() uint16` -- Returns the fixed port number (0 for `auto`).
- `IsAuto() bool` -- Returns `true` for the `"auto"` form.

## Decoding patterns

- **Map for services** -- Services are stored in a
  `map[string]ServiceConfig`. Where deterministic iteration order matters
  (output, logging, the dependency graph), devrig sorts the keys explicitly.
- **Pointers and zero values** -- Optional blocks use pointer fields (e.g.
  `*Port`, `*ComposeConfig`) so an omitted section decodes to `nil`; omitted
  scalar fields fall back to their Go zero value.
- **Custom unmarshalers** -- `Port` and `StringOrList` implement
  `UnmarshalTOML`/`UnmarshalYAML` so the same key can accept either a scalar or
  a list/string form, which struct-tag decoding alone cannot express.
