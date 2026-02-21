# ADR 002: No profile system -- use -f flag instead

## Status

Accepted

## Context

Development teams often need to run different subsets of services or different
configurations for different environments. Common approaches include:

- **Profile systems** (e.g. Docker Compose profiles): services are tagged with
  profile names, and users select which profiles to activate at runtime.
- **File-based overrides** (e.g. `docker-compose.override.yml`): separate
  files are merged at load time.
- **Explicit file selection**: the user points the tool at a specific config
  file.

Profile systems add hidden complexity. Users must remember which services
belong to which profiles, profiles interact with each other in non-obvious
ways, and the resulting runtime config becomes difficult to reason about.

## Decision

devrig does not have a profile system. Instead, users pass the `-f` flag to
select a specific configuration file:

```bash
devrig start -f devrig.staging.toml
```

Each file is a complete, self-contained configuration. There is no merging,
inheritance, or override mechanism.

## Consequences

**Positive:**

- Simpler mental model: "one file = one environment." Users can read the file
  and know exactly what will run.
- Explicit over implicit. No hidden profile activation rules or merge
  semantics.
- Easy to diff two environments: `diff devrig.toml devrig.staging.toml`.
- Less code to maintain: no profile resolution, tagging, or merge logic.

**Negative:**

- Some duplication between config files when environments share most services.
  This is acceptable for typical project sizes (3-10 services).
- Users familiar with Docker Compose profiles may initially look for an
  equivalent feature.

**Mitigations:**

- Users who want to reduce duplication can use external templating tools or
  generate TOML files from a script.
- The `devrig start <service...>` syntax allows starting a subset of services
  from a single config without needing profiles.
