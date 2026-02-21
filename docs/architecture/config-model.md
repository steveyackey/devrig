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

The TOML is deserialized into these Rust types (defined in `config/model.rs`):

```rust
struct DevrigConfig {
    project: ProjectConfig,
    services: BTreeMap<String, ServiceConfig>,  // Ordered by name
    env: BTreeMap<String, String>,
}

struct ProjectConfig {
    name: String,
}

struct ServiceConfig {
    path: Option<String>,
    command: String,
    port: Option<Port>,
    env: BTreeMap<String, String>,
    depends_on: Vec<String>,
}

enum Port {
    Fixed(u16),
    Auto,
}
```

## Two-phase parsing

Configuration loading happens in two distinct phases:

### Phase 1: Deserialization

The `toml` crate deserializes the file into `DevrigConfig`. Structural errors
(missing required fields, wrong types, malformed TOML) are caught here. The
`toml` crate provides line and column numbers in error messages.

### Phase 2: Semantic validation

The `validate()` function in `config/validate.rs` performs cross-field checks
that serde cannot express:

1. **Dependency existence** -- Every entry in `depends_on` must reference a
   service name that exists in `[services.*]`.
2. **Duplicate port detection** -- No two services may declare the same fixed
   port number.
3. **Cycle detection** -- The dependency graph must be a DAG. Cycles are
   detected via iterative DFS with a visited/in-stack approach.
4. **Empty command check** -- The `command` field must not be blank or
   whitespace-only.

All errors are collected into a `Vec<ConfigError>` and reported together,
rather than failing on the first error. This lets users fix multiple issues
in a single edit cycle.

## Port type design

The `Port` enum supports two representations in TOML:

```toml
port = 3000     # Fixed port: Port::Fixed(3000)
port = "auto"   # Auto-assign: Port::Auto
                 # (omitted): Option<Port> is None
```

This is implemented via a custom `Deserialize` implementation using a
`Visitor` that handles both `visit_u64`/`visit_i64` (for integers) and
`visit_str` (for the `"auto"` string). The `deserialize_any` method is used
so TOML can present the value in whatever native type it parsed.

Range validation happens inside the visitor: integers outside 1-65535 produce
a descriptive error. Strings other than `"auto"` are rejected.

Helper methods on `Port`:
- `as_fixed() -> Option<u16>` -- Returns `Some(port)` for `Fixed`, `None` for
  `Auto`.
- `is_auto() -> bool` -- Returns `true` for `Auto`.

## Serde patterns

- **BTreeMap for services** -- Services are stored in a `BTreeMap<String, ServiceConfig>` rather than `HashMap` to ensure deterministic iteration
  order (alphabetical by service name). This makes output, logging, and tests
  predictable.
- **#[serde(default)]** -- Optional fields use `#[serde(default)]` so that
  omitting them in TOML produces the Rust default (`None`, empty `Vec`, empty
  `BTreeMap`).
- **Custom visitor for Port** -- A manual `Visitor` implementation allows the
  same TOML key to accept both integers and strings, which serde's derive
  macros cannot express with `#[serde(untagged)]` for primitive types in TOML.
