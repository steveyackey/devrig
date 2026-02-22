# Screenshot Skill

Regenerate dashboard screenshots for documentation.

## When to use

Use this skill when:
- Dashboard UI has been changed and screenshots need updating
- The user asks to take/update/regenerate screenshots
- After visual changes to the dashboard

## Prerequisites

- devrig must be running (either via `devrig start` or the production binary)
- The dashboard must be accessible at `http://localhost:4000`

## Steps

1. **Build the dashboard and binary** (skip if already running with latest changes):
   ```bash
   cd dashboard && bun run build
   touch src/dashboard/static_files.rs && cargo build
   ```

2. **Start devrig** if not already running:
   ```bash
   cargo run -- start -f devrig.run.toml
   ```

3. **Run the screenshot script** — this seeds OTLP telemetry data, captures
   screenshots of every dashboard view, and updates README.md image references:
   ```bash
   cd e2e && bun run screenshots
   ```

4. **Stage changes** — use `git add -u docs/images/` (not a glob) so both new
   files and deletions of old hashed filenames are staged:
   ```bash
   git add -u docs/images/
   git add README.md
   ```

5. **Verify** the updated screenshots look correct by listing the new files:
   ```bash
   git status docs/images/
   ```
