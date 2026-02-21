# ADR 006: In-memory ring buffers for OpenTelemetry data

## Status

Accepted

## Context

devrig aims to provide an Aspire-style observability dashboard for local
development, showing traces, logs, and metrics from running services via
OpenTelemetry (OTel).

Production observability stacks persist telemetry data to disk or to external
backends (Jaeger, Prometheus, Loki). For local development, this persistence
introduces problems:

- Disk usage accumulates over time and requires manual cleanup.
- External backends (even containerized ones) consume significant memory and
  CPU.
- Startup time increases when backends need to initialize storage.
- Data from previous sessions is rarely useful and often confusing.

## Decision

Store all OpenTelemetry data in in-memory ring buffers with a fixed capacity.
When the buffer is full, the oldest entries are evicted. No data is written
to disk.

The ring buffer sizes will be tunable via configuration but ship with sensible
defaults (e.g. last 10,000 spans, last 50,000 log lines, last 5 minutes of
metrics at 10-second resolution).

## Consequences

**Positive:**

- Fast: no disk I/O, no database queries. The dashboard reads directly from
  memory.
- No cleanup needed: stopping devrig frees all telemetry data. No leftover
  files, no growing disk usage.
- Predictable memory usage: ring buffer capacity sets an upper bound.
- Instant startup: no backend initialization or migration step.

**Negative:**

- All telemetry data is lost when devrig restarts. This is acceptable for
  local development where the current session is what matters.
- Cannot query historical data across sessions. If needed, users can export
  to a real backend.

**Neutral:**

- The OTel collector endpoint is still standards-compliant. Services configure
  their OTel SDK to point at devrig's collector, and if they later switch to
  a production backend, no code changes are needed.
