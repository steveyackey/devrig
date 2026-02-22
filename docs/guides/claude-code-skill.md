# Claude Code Skill

devrig includes a built-in [Claude Code](https://claude.com/claude-code) skill
that gives Claude access to your local development environment's observability
data. Once installed, Claude can query traces, logs, and metrics from your
running services.

## Installation

Install the skill for the current project:

```bash
devrig skill install
```

This writes the skill file to `.claude/skills/devrig/SKILL.md` relative to
your project root (the directory containing `devrig.toml`).

To install globally (available in all projects):

```bash
devrig skill install --global
```

Global install writes to `~/.claude/skills/devrig/SKILL.md`.

## What the skill provides

The skill grants Claude Code access to all `devrig` CLI commands via
`Bash(devrig *)`. This includes:

### Query commands

- **`devrig query traces`** -- List recent traces with filters for service,
  status, and duration.
- **`devrig query trace <id>`** -- Inspect a specific trace's span waterfall.
- **`devrig query logs`** -- Search logs by service, level, text, or trace ID.
- **`devrig query metrics`** -- Query collected metrics by name or service.
- **`devrig query status`** -- Check collector health and telemetry counts.
- **`devrig query related <id>`** -- Get logs and metrics correlated with a
  trace.

### Service management

- **`devrig ps`** -- List running services and their status.
- **`devrig env <service>`** -- Show resolved environment variables.
- **`devrig k <args>`** -- Run kubectl against the devrig cluster.

## Example conversations

With the skill installed, you can ask Claude things like:

- *"Why is the API slow?"* -- Claude will query traces for
  high-duration spans, inspect the span waterfall, and check related logs.
- *"Are there any errors in the last 10 minutes?"* -- Claude will search
  error traces and logs across all services.
- *"What's the health of my system?"* -- Claude will check collector status
  and look for warnings.
- *"Show me the logs for the auth service"* -- Claude will query logs
  filtered by service name.

## How it works

The skill file uses Claude Code's YAML frontmatter to declare:

- **name**: `devrig`
- **description**: Triggers automatic activation when the user asks about
  service health, errors, traces, logs, or debugging.
- **allowed-tools**: `Bash(devrig *)` -- permits Claude to run any `devrig`
  subcommand.

Claude Code loads the skill and follows the included workflow guides for
debugging performance issues, investigating errors, and checking system health.

## Output formats

All query commands support `--format` to control output:

```bash
devrig query traces --format table     # Terminal-friendly table
devrig query traces --format json      # Pretty-printed JSON
devrig query traces --format jsonl     # NDJSON, one object per line
```

Default output is NDJSON, which is well-suited for piping to `jq`. Claude
Code uses this format to parse and analyze results programmatically.

## Prerequisites

The skill requires a running devrig environment with the dashboard enabled.
Make sure your `devrig.toml` includes a `[dashboard]` section:

```toml
[dashboard]
port = 4000
```

Start your services with `devrig start`, then use Claude Code as usual.
The skill activates automatically when you ask about observability topics.
