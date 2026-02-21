# ADR 001: TOML as the sole configuration format

## Status

Accepted

## Context

Multiple configuration formats are commonly used in developer tooling: YAML,
JSON, TOML, HCL, and others. Projects like Docker Compose use YAML, Terraform
uses HCL, and many Rust tools use TOML.

We need to choose a configuration format for `devrig.toml`. The choice affects
error messages, editor support, the learning curve for users, and maintenance
burden on the parser side.

Supporting multiple formats (e.g. both YAML and TOML) would increase the
surface area for bugs, require additional test coverage, and complicate
documentation since every example would need to be shown in each format.

## Decision

Use TOML exclusively for all devrig configuration. The config file is always
named `devrig.toml`. No alternative formats are supported or planned.

The `toml` crate (v0.8) is used for deserialization into strongly typed Rust
structs via serde. A custom `Deserialize` implementation is used for the `Port`
type to accept either an integer or the string `"auto"`.

## Consequences

**Positive:**

- Familiar to Rust developers who already use `Cargo.toml` daily.
- The `toml` crate produces clear, line-numbered error messages when
  deserialization fails, making misconfiguration easy to diagnose.
- No YAML gotchas (the Norway problem, implicit type coercion, significant
  whitespace).
- Single format means every documentation example works as-is.
- Simpler codebase: one parser, one set of tests.

**Negative:**

- Users coming from Docker Compose may expect YAML support.
- TOML's table syntax can be verbose for deeply nested structures, though
  devrig's config is intentionally shallow.

**Neutral:**

- If demand for alternative formats arises, an external `devrig-yaml` converter
  could be built without changing the core.
