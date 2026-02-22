---
name: devrig
description: Inspect and debug a running devrig development environment. Use when the user asks about service health, errors, performance, traces, logs, metrics, debugging, slow requests, what happened at a specific time, resource status, or anything related to their local dev environment managed by devrig.
allowed-tools:
  - Bash(devrig *)
---

# devrig — Local Development Environment Observability

You have access to a running devrig environment with built-in OpenTelemetry collection. Use the commands below to investigate issues, check health, and correlate telemetry data.

## Query Commands

### Traces

List recent traces with optional filters:
```bash
devrig query traces --service <name> --status <ok|error> --min-duration <ms> --limit <n> --format <table|json|jsonl>
```

Get full detail for a specific trace (span waterfall):
```bash
devrig query trace <trace-id> --format <table|json|jsonl>
```

Get related logs and metrics for services involved in a trace:
```bash
devrig query related <trace-id> --format <table|json|jsonl>
```

### Logs

Search logs from the OTel collector:
```bash
devrig query logs --service <name> --level <trace|debug|info|warn|error|fatal> --search <text> --trace-id <id> --limit <n> --format <table|json|jsonl>
```

### Metrics

Query collected metrics:
```bash
devrig query metrics --name <metric-name> --service <name> --limit <n> --format <table|json|jsonl>
```

### Status

Check the OTel collector status (trace/log/metric counts, reporting services):
```bash
devrig query status --format <table|json>
```

## Service Management Commands

List running services with ports and status:
```bash
devrig ps
```

Show resolved environment variables for a service:
```bash
devrig env <service-name>
```

Restart a service (via dashboard API):
```bash
devrig kubectl rollout restart deployment/<name>
```

Proxy to kubectl with devrig's isolated kubeconfig:
```bash
devrig k get pods
devrig k get services
devrig k logs <pod-name>
```

## Output Format

- Default output is NDJSON (one JSON object per line), suitable for piping to `jq`
- Use `--format json-pretty` for human-readable JSON (alias: `--output json-pretty`)
- Use `--format table` for terminal-friendly table output
- All query commands support `--limit` to control result count

## Workflow: Debugging Performance Issues

1. **Find slow traces** — identify traces exceeding a duration threshold:
   ```bash
   devrig query traces --min-duration 500 --limit 10
   ```

2. **Inspect the slow trace** — look at the span waterfall to find the bottleneck:
   ```bash
   devrig query trace <trace-id>
   ```

3. **Check related telemetry** — get logs and metrics from services involved:
   ```bash
   devrig query related <trace-id>
   ```

4. **Search for error logs** in the slow service:
   ```bash
   devrig query logs --service <service-name> --level error --limit 20
   ```

## Workflow: Investigating Errors

1. **Find error traces**:
   ```bash
   devrig query traces --status error --limit 10
   ```

2. **Get error details** from the trace:
   ```bash
   devrig query trace <trace-id>
   ```

3. **Search error logs** across all services:
   ```bash
   devrig query logs --level error --limit 30
   ```

4. **Narrow to a specific service**:
   ```bash
   devrig query logs --service <service-name> --level warn --search "timeout" --limit 20
   ```

5. **Cross-reference** with related telemetry:
   ```bash
   devrig query related <trace-id>
   ```

## Workflow: Checking System Health

1. **Get overall status** — see what's reporting and counts:
   ```bash
   devrig query status
   ```

2. **Check metrics** for anomalies:
   ```bash
   devrig query metrics --limit 50
   ```

3. **Look for warnings and errors** across all services:
   ```bash
   devrig query logs --level warn --limit 30
   ```

4. **Check specific service health**:
   ```bash
   devrig query traces --service <service-name> --limit 10
   devrig query logs --service <service-name> --level error
   ```

## Tips

- Trace IDs can be passed as full hex strings or prefixes
- Use `jq` for complex filtering: `devrig query traces --format jsonl | jq 'select(.has_error == true)'`
- The dashboard UI is available at the URL shown in `devrig ps` output
- All telemetry is stored in-memory with configurable retention (default 1h)
- Services auto-inject `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_SERVICE_NAME`
