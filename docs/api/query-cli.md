# CLI Query Reference

The `devrig query` command provides CLI access to telemetry data collected
by the built-in OpenTelemetry collector. It communicates with the dashboard
REST API, so the dashboard must be running (`devrig start` with dashboard
enabled).

All subcommands support `--output` (`-o`) for choosing the output format:
`table` (default), `json`, or `jsonl`.

## Subcommands

### `devrig query traces`

List traces with optional filters.

**Flags:**

| Flag              | Short | Type    | Default | Description                         |
|-------------------|-------|---------|---------|-------------------------------------|
| `--service`       | `-s`  | string  | (none)  | Filter traces by service name       |
| `--status`        |       | string  | (none)  | Filter by status: `ok` or `error`   |
| `--min-duration`  |       | integer | (none)  | Minimum trace duration in ms        |
| `--limit`         | `-n`  | integer | `20`    | Maximum number of results           |
| `--output`        | `-o`  | string  | `table` | Output format: `table`, `json`, `jsonl` |

**Examples:**

```bash
# List the 20 most recent traces
devrig query traces

# Filter by service and show as JSON
devrig query traces --service api --output json

# Find slow traces (>500ms) with errors
devrig query traces --status error --min-duration 500

# Limit results
devrig query traces --limit 5
```

**Table output:**

```
  +------------------+-----------+----------+----------+-------+--------+
  | Trace ID         | Operation | Services | Duration | Spans | Status |
  +------------------+-----------+----------+----------+-------+--------+
  | a1b2c3d4e5f6a7b8 | GET /users| api, web |    142ms |     5 |   OK   |
  | c9d0e1f2a3b4c5d6 | POST /pay | api      |   1.2s   |     3 | ERROR  |
  +------------------+-----------+----------+----------+-------+--------+
```

---

### `devrig query trace <TRACE_ID>`

Get the detail of a specific trace, showing all its spans.

**Arguments:**

| Argument   | Required | Description                   |
|------------|----------|-------------------------------|
| `TRACE_ID` | Yes      | Full or prefix trace ID       |

**Flags:**

| Flag       | Short | Type   | Default | Description                         |
|------------|-------|--------|---------|-------------------------------------|
| `--output` | `-o`  | string | `table` | Output format: `table`, `json`, `jsonl` |

**Examples:**

```bash
# Show spans for a trace in table format
devrig query trace a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6

# Output as JSON for piping
devrig query trace a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6 --output json
```

**Table output:**

```
  +------------------+---------+------------+----------+--------+--------+
  | Span ID          | Service | Operation  | Duration |  Kind  | Status |
  +------------------+---------+------------+----------+--------+--------+
  | 1a2b3c4d5e6f7a8b | api     | GET /users |    142ms | Server |   OK   |
  | 2b3c4d5e6f7a8b9c | api     | db.query   |     23ms | Client |   OK   |
  +------------------+---------+------------+----------+--------+--------+
```

**Error:** exits with an error message if the trace is not found.

---

### `devrig query logs`

Query log records from the OTel collector.

**Flags:**

| Flag         | Short | Type    | Default | Description                                        |
|--------------|-------|---------|---------|----------------------------------------------------|
| `--service`  | `-s`  | string  | (none)  | Filter by service name                              |
| `--severity` | `-l`  | string  | (none)  | Minimum severity: `trace`, `debug`, `info`, `warn`, `error`, `fatal` |
| `--search`   | `-g`  | string  | (none)  | Case-insensitive text search in log body            |
| `--trace-id` |       | string  | (none)  | Filter logs by associated trace ID                  |
| `--limit`    | `-n`  | integer | `50`    | Maximum number of results                           |
| `--output`   | `-o`  | string  | `table` | Output format: `table`, `json`, `jsonl`             |

**Examples:**

```bash
# List recent logs
devrig query logs

# Filter by service and severity
devrig query logs --service api --severity error

# Search log bodies
devrig query logs --search "connection refused"

# Find logs for a specific trace
devrig query logs --trace-id a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6

# Combine filters with JSON output
devrig query logs --service api --severity warn --limit 100 --output json
```

**Table output:**

```
  +--------------+---------+-------+---------------------------------------------+
  | Time         | Service | Level | Body                                        |
  +--------------+---------+-------+---------------------------------------------+
  | 10:30:00.050 | api     | Error | database connection failed: timeout after 5s|
  | 10:30:01.200 | web     | Info  | Request completed successfully              |
  +--------------+---------+-------+---------------------------------------------+
```

Log bodies longer than 120 characters are truncated in table output. Use
`--output json` to see full bodies.

---

### `devrig query metrics`

Query metric data points from the OTel collector.

**Flags:**

| Flag        | Short | Type    | Default | Description                         |
|-------------|-------|---------|---------|-------------------------------------|
| `--name`    | `-m`  | string  | (none)  | Filter by metric name               |
| `--service` | `-s`  | string  | (none)  | Filter by service name              |
| `--limit`   | `-n`  | integer | `50`    | Maximum number of results           |
| `--output`  | `-o`  | string  | `table` | Output format: `table`, `json`, `jsonl` |

**Examples:**

```bash
# List recent metrics
devrig query metrics

# Filter by metric name
devrig query metrics --name http.server.duration

# Filter by service
devrig query metrics --service api --limit 100

# Output as JSONL for streaming processing
devrig query metrics --output jsonl
```

**Table output:**

```
  +--------------+---------+----------------------+-----------+--------+------+
  | Time         | Service | Metric               |   Type    |  Value | Unit |
  +--------------+---------+----------------------+-----------+--------+------+
  | 10:30:00.100 | api     | http.server.duration | Histogram |    142 | ms   |
  | 10:30:00.100 | api     | http.server.count    | Counter   |     57 | -    |
  +--------------+---------+----------------------+-----------+--------+------+
```

---

### `devrig query status`

Show the current status of the OTel collector, including counts of stored
telemetry and the list of reporting services.

**Flags:**

| Flag       | Short | Type   | Default | Description                         |
|------------|-------|--------|---------|-------------------------------------|
| `--output` | `-o`  | string | `table` | Output format: `table` or `json`    |

**Examples:**

```bash
# Show collector status
devrig query status

# Get status as JSON for scripting
devrig query status --output json
```

**Table output:**

```
  OTel Collector Status

  Traces:  312
  Spans:   1523
  Logs:    4210
  Metrics: 8934

  Services: api, web, worker
```

**JSON output:**

```json
{
  "span_count": 1523,
  "log_count": 4210,
  "metric_count": 8934,
  "services": ["api", "web", "worker"],
  "trace_count": 312
}
```

## Common patterns

### Investigate a slow request

```bash
# Find slow traces
devrig query traces --min-duration 1000

# Get the span breakdown
devrig query trace a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6

# Check for related error logs
devrig query logs --trace-id a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6 --severity error
```

### Monitor error rates

```bash
# List traces with errors
devrig query traces --status error --limit 50

# Check error logs across all services
devrig query logs --severity error
```

### Export telemetry for analysis

```bash
# Export all traces as JSON
devrig query traces --limit 1000 --output json > traces.json

# Stream logs as JSONL
devrig query logs --limit 10000 --output jsonl > logs.jsonl
```

## Requirements

The `devrig query` commands require a running devrig instance with the
dashboard enabled. If the dashboard is not running, the commands will
exit with an error:

```
Error: no running project found -- is devrig start running?
```

If the dashboard is disabled in the configuration:

```
Error: dashboard is not enabled in this project
```
