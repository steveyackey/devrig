# OTel Storage Architecture

This document describes the in-memory telemetry storage engine used by
devrig's built-in OpenTelemetry collector.

## Overview

devrig stores all received telemetry (spans, logs, metrics) in memory using
a ring buffer design. There is no on-disk persistence -- data lives only for
the configured retention period or until the buffer capacity is reached.
This keeps the architecture simple and avoids external dependencies like
databases or file-based stores.

The storage is implemented in `src/otel/storage.rs` as `TelemetryStore`.

## Ring buffer design

Each telemetry type has its own ring buffer backed by a `VecDeque` with
manual capacity enforcement:

```
spans:   VecDeque<StoredSpan>     (default capacity: 10,000)
logs:    VecDeque<StoredLog>      (default capacity: 100,000)
metrics: VecDeque<StoredMetric>   (default capacity: 50,000)
```

Buffer sizes are configurable per type through the `[dashboard.otel]`
configuration section:

| Config field    | Default  | Description                       |
|-----------------|----------|-----------------------------------|
| `trace_buffer`  | `10000`  | Maximum number of spans stored    |
| `metric_buffer` | `50000`  | Maximum number of metrics stored  |
| `log_buffer`    | `100000` | Maximum number of log records     |

Initial `VecDeque` allocation is capped at 65,536 entries via
`with_capacity(max.min(65536))` to avoid excessive up-front allocation when
large limits are configured.

## Record IDs

Every inserted record (span, log, or metric) is assigned a monotonically
increasing `u64` record ID via a shared `next_id` counter. Record IDs are
used as keys in all secondary indexes, providing a stable reference that
does not change as the ring buffer shifts.

## Secondary indexes

To support fast querying without linear scans, the store maintains several
secondary indexes alongside the primary ring buffers:

### Span indexes

| Index                | Type                          | Purpose                              |
|----------------------|-------------------------------|--------------------------------------|
| `trace_index`        | `HashMap<String, Vec<u64>>`   | Maps `trace_id` to span record IDs   |
| `service_span_index` | `HashMap<String, Vec<u64>>`   | Maps `service_name` to span IDs      |
| `error_spans`        | `HashSet<u64>`                | Set of record IDs with Error status  |

The `trace_index` is the primary lookup structure for trace detail queries
and the related-telemetry endpoint. When a trace ID is requested, the index
returns the record IDs of all spans belonging to that trace without
scanning the full buffer.

### Log indexes

| Index               | Type                          | Purpose                            |
|---------------------|-------------------------------|------------------------------------|
| `service_log_index` | `HashMap<String, Vec<u64>>`   | Maps `service_name` to log IDs     |

### Metric indexes

| Index                  | Type                          | Purpose                              |
|------------------------|-------------------------------|--------------------------------------|
| `service_metric_index` | `HashMap<String, Vec<u64>>`   | Maps `service_name` to metric IDs    |

### Index maintenance

Indexes are updated on every insert. When a record is evicted (either by
capacity or retention sweep), the corresponding entries are removed from
all relevant indexes. If removing a record ID leaves an index entry with
an empty `Vec`, the entire key is removed from the `HashMap` to prevent
unbounded index growth.

## Eviction strategy

Records are evicted in two ways:

### 1. Capacity-based eviction (pop_front)

When a ring buffer reaches its configured maximum size, the oldest entry
is evicted via `pop_front()` before the new entry is inserted:

```rust
if self.spans.len() >= self.max_spans {
    if let Some(evicted) = self.spans.pop_front() {
        self.remove_span_from_indexes(&evicted);
    }
}
```

This maintains a strict upper bound on memory usage per buffer type.

### 2. Time-based retention (background sweeper)

A background tokio task runs every 30 seconds and calls `sweep_expired()`
on the store. This method walks the front of each ring buffer and removes
all entries whose timestamp is older than the retention cutoff:

```rust
let cutoff = Utc::now() - retention;

while let Some(front) = self.spans.front() {
    if front.start_time < cutoff {
        let evicted = self.spans.pop_front().unwrap();
        self.remove_span_from_indexes(&evicted);
    } else {
        break;
    }
}
```

Because entries are inserted in chronological order, the sweep can stop as
soon as it encounters a record within the retention window. This makes the
sweep O(k) where k is the number of expired records, not O(n) over the
full buffer.

The sweeper is cancelled cleanly via a `CancellationToken` on shutdown.

### Retention configuration

The retention duration is configured as a human-readable string parsed by
the `humantime` crate:

```toml
[dashboard.otel]
retention = "1h"      # default
retention = "30m"     # 30 minutes
retention = "2h30m"   # 2 hours 30 minutes
```

If the retention string fails to parse, it falls back to 3600 seconds (1 hour).

## Concurrency model

The `TelemetryStore` is wrapped in `tokio::sync::RwLock` and shared via
`Arc`:

```rust
store: Arc<RwLock<TelemetryStore>>
```

This provides the following concurrency characteristics:

- **Multiple concurrent readers**: All dashboard REST API handlers and
  WebSocket broadcasts acquire a read lock (`store.read().await`). Multiple
  API requests can query the store simultaneously without blocking each
  other.

- **Brief exclusive writes**: The gRPC and HTTP OTLP receivers acquire a
  write lock (`store.write().await`) only for the duration of inserting
  received telemetry. The background sweeper also takes a write lock, but
  only every 30 seconds and only for the time needed to evict expired
  entries.

- **No lock contention in practice**: Write locks are held for very short
  durations (microseconds for a single insert). The 30-second sweep
  interval means the sweeper rarely competes with insert operations.

### Lock holders

| Component           | Lock type | Frequency             |
|---------------------|-----------|-----------------------|
| REST API handlers   | Read      | Per HTTP request      |
| WebSocket broadcast | Read      | Per event             |
| gRPC receiver       | Write     | Per batch of spans    |
| HTTP receiver       | Write     | Per batch of spans    |
| Background sweeper  | Write     | Every 30 seconds      |

## Memory model

All telemetry is stored in Rust structs on the heap. There is no
serialization to disk or memory-mapped storage. The approximate memory
budget is:

```
Total ~= (trace_buffer * avg_span_size)
       + (log_buffer * avg_log_size)
       + (metric_buffer * avg_metric_size)
       + index overhead
```

With default settings (10k spans, 100k logs, 50k metrics), the store
typically uses 50-200 MB depending on attribute density.

### Stored types

Each record type carries the following fields:

**StoredSpan**: record_id, trace_id, span_id, parent_span_id, service_name,
operation_name, start_time, end_time, duration_ms, status, status_message,
attributes (up to 20 key-value pairs), kind.

**StoredLog**: record_id, timestamp, service_name, severity, body, trace_id,
span_id, attributes (up to 20 key-value pairs).

**StoredMetric**: record_id, timestamp, service_name, metric_name,
metric_type, value, attributes (up to 20 key-value pairs), unit.

## Query layer

The `TelemetryStore` exposes query methods (defined in `src/otel/query.rs`)
that operate on the ring buffers and indexes:

- `query_traces()` -- groups spans by trace_id, applies service/status/duration
  filters, returns `TraceSummary` sorted by most recent first.
- `get_trace()` -- uses `trace_index` to find all spans for a trace ID.
- `query_logs()` -- reverse iteration with service/severity/search/trace_id
  filters.
- `query_metrics()` -- reverse iteration with name/service filters.
- `get_status()` -- returns counts and service list.
- `get_related()` -- finds logs and metrics from the same services within
  the time window of a trace, plus a 5-second buffer.

All query methods only acquire a read lock and return owned (cloned) data,
so results remain valid after the lock is released.
