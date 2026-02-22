# CLAUDE.md — devrig

Local development orchestrator. Rust CLI + SolidJS dashboard.

## Build & Run

```bash
cd dashboard && bun run build    # build dashboard frontend
touch src/dashboard/static_files.rs && cargo build  # re-embed frontend into binary
cargo run -- start               # start from devrig.toml
```

Frontend assets are embedded at compile time via `rust_embed`. After changing
dashboard code you must rebuild both the frontend AND the Rust binary — just
`cargo build` alone won't pick up `dashboard/dist/` changes (touch the embed
file to force recompile).

## Tests

```bash
cd e2e && bunx playwright test    # 79 dashboard E2E tests (requires running devrig)
cd e2e && bun run screenshots    # regenerate docs/images/ screenshots (on-demand, seeds OTLP data)
```

Screenshot tests are excluded from the default test run. They seed telemetry
via OTLP HTTP, then capture each view. To update screenshots after UI changes:

```bash
cd dashboard && bun run build
touch src/dashboard/static_files.rs && cargo build
cd e2e && bun run screenshots
```

## Project Structure

- `src/` — Rust: orchestrator, dashboard server, OTel collector
- `dashboard/` — SolidJS + Tailwind v4 + Vite frontend
- `e2e/` — Playwright E2E tests
- `docs/images/` — Dashboard screenshots (auto-generated)

## Conventions

- Tailwind v4 (CSS-based config, no tailwind.config.js)
- Biome for frontend linting (not ESLint)
- Dashboard state served from `.devrig/state.json`
- OTLP HTTP default port: 4318, gRPC: 4317, dashboard: 4000

## Package Manager

Use **bun**, not npm. All package management and script running uses bun.
