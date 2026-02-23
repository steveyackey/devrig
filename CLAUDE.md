# CLAUDE.md — devrig

Local development orchestrator. Rust CLI + SolidJS dashboard.

## Dev Workflow (Hot Reload)

```bash
cargo watch -w src -w Cargo.toml -s 'cargo run -- start --dev -f devrig.run.toml'
open localhost:5173   # hot-reloaded frontend, proxied API
```

- Edit `dashboard/src/**` → instant HMR via Vite on `:5173`
- Edit `src/**` → cargo watch rebuilds + restarts the API (~5s)
- Vite dev server on `:5173` proxies `/api` and `/ws` to the Rust server on `:4000`
- `--dev` spawns Vite automatically (debug builds only, hidden from help)

Screenshot tests auto-detect Vite on `:5173` and use it as the base URL,
so `bun run screenshots` captures live source changes without a build step.

## Production Build

```bash
cd dashboard && bun run build    # build dashboard frontend
touch src/dashboard/static_files.rs && cargo build  # re-embed frontend into binary
cargo run -- start -f devrig.run.toml  # start with minimal config
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

The screenshot script (`bun run screenshots`) deletes old hashed images from
`docs/images/` and creates new ones, then `update-readme-hashes.ts` rewrites
README.md references. When committing, use `git add -u docs/images/` (not a
glob) so that both the new files **and** the deletions of old files are staged.

## Project Structure

- `src/` — Rust: orchestrator, dashboard server, OTel collector
- `dashboard/` — SolidJS + Tailwind v4 + Vite frontend
- `e2e/` — Playwright E2E tests
- `docs/images/` — Dashboard screenshots (auto-generated)

## Conventions

- Conventional commits (e.g. `feat:`, `fix:`, `chore:`, `docs:`)
- Tailwind v4 (CSS-based config, no tailwind.config.js)
- Biome for frontend linting (not ESLint)
- Dashboard state served from `.devrig/state.json`
- OTLP HTTP default port: 4318, gRPC: 4317, dashboard: 4000

## Package Manager

Use **bun**, not npm. All package management and script running uses bun.
