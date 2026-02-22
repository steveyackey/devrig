# REST API Reference

The devrig dashboard exposes a REST API for querying telemetry data
collected by the built-in OpenTelemetry collector. The API is served on the
dashboard port (default `4000`).

All endpoints return JSON. Error responses use standard HTTP status codes.

## Endpoints

### GET /api/traces

List traces with optional filters. Returns an array of trace summaries
sorted by start time (most recent first).

**Query parameters:**

| Parameter        | Type    | Default | Description                        |
|------------------|---------|---------|------------------------------------|
| `service`        | string  | (none)  | Filter traces by service name      |
| `status`         | string  | (none)  | Filter by status: `ok` or `error`  |
| `min_duration_ms`| integer | (none)  | Minimum trace duration in ms       |
| `limit`          | integer | `100`   | Maximum number of results          |

**Example request:**

```bash
curl "http://localhost:4000/api/traces?service=api&status=error&limit=10"
```

**Example response:**

```json
[
  {
    "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    "services": ["api", "web"],
    "root_operation": "GET /users",
    "duration_ms": 142,
    "span_count": 5,
    "has_error": true,
    "start_time": "2026-02-22T10:30:00.000Z"
  }
]
```

**Response fields:**

| Field            | Type     | Description                                     |
|------------------|----------|-------------------------------------------------|
| `trace_id`       | string   | Full hex-encoded trace ID                       |
| `services`       | string[] | Sorted list of unique service names in the trace|
| `root_operation` | string   | Operation name of the root span                 |
| `duration_ms`    | integer  | Total trace duration in milliseconds            |
| `span_count`     | integer  | Number of spans in the trace                    |
| `has_error`      | boolean  | Whether any span has Error status               |
| `start_time`     | string   | ISO 8601 timestamp of the trace start           |

---

### GET /api/traces/{trace_id}

Get the full detail of a specific trace, including all spans.

**Path parameters:**

| Parameter  | Type   | Description     |
|------------|--------|-----------------|
| `trace_id` | string | Full trace ID   |

**Example request:**

```bash
curl "http://localhost:4000/api/traces/a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"
```

**Example response:**

```json
{
  "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
  "spans": [
    {
      "record_id": 42,
      "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
      "span_id": "1a2b3c4d5e6f7a8b",
      "parent_span_id": null,
      "service_name": "api",
      "operation_name": "GET /users",
      "start_time": "2026-02-22T10:30:00.000Z",
      "end_time": "2026-02-22T10:30:00.142Z",
      "duration_ms": 142,
      "status": "Ok",
      "status_message": null,
      "attributes": [
        ["http.method", "GET"],
        ["http.url", "/users"]
      ],
      "kind": "Server"
    }
  ]
}
```

**Span fields:**

| Field             | Type            | Description                                |
|-------------------|-----------------|--------------------------------------------|
| `record_id`       | integer         | Internal record ID                         |
| `trace_id`        | string          | Hex-encoded trace ID                       |
| `span_id`         | string          | Hex-encoded span ID                        |
| `parent_span_id`  | string or null  | Parent span ID, null for root spans        |
| `service_name`    | string          | Reporting service name                     |
| `operation_name`  | string          | Span operation name                        |
| `start_time`      | string          | ISO 8601 start timestamp                   |
| `end_time`        | string          | ISO 8601 end timestamp                     |
| `duration_ms`     | integer         | Span duration in milliseconds              |
| `status`          | string          | `Ok`, `Error`, or `Unset`                  |
| `status_message`  | string or null  | Optional status message                    |
| `attributes`      | [string, string][] | Key-value attribute pairs (max 20)      |
| `kind`            | string          | `Internal`, `Server`, `Client`, `Producer`, or `Consumer` |

**Error response:**

Returns `404 Not Found` if the trace ID does not exist.

---

### GET /api/traces/{trace_id}/related

Get logs and metrics related to a specific trace. Returns telemetry from
the same services within the trace's time window (plus a 5-second buffer
on each side).

**Path parameters:**

| Parameter  | Type   | Description     |
|------------|--------|-----------------|
| `trace_id` | string | Full trace ID   |

**Example request:**

```bash
curl "http://localhost:4000/api/traces/a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6/related"
```

**Example response:**

```json
{
  "logs": [
    {
      "record_id": 101,
      "timestamp": "2026-02-22T10:30:00.050Z",
      "service_name": "api",
      "severity": "Info",
      "body": "Fetching users from database",
      "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
      "span_id": "1a2b3c4d5e6f7a8b",
      "attributes": []
    }
  ],
  "metrics": [
    {
      "record_id": 202,
      "timestamp": "2026-02-22T10:30:00.100Z",
      "service_name": "api",
      "metric_name": "http.server.duration",
      "metric_type": "Histogram",
      "value": 142.0,
      "attributes": [
        ["http.method", "GET"]
      ],
      "unit": "ms"
    }
  ]
}
```

If the trace ID does not exist, returns an empty result:

```json
{
  "logs": [],
  "metrics": []
}
```

---

### GET /api/logs

Query log records from the OTel collector.

**Query parameters:**

| Parameter  | Type    | Default | Description                                       |
|------------|---------|---------|---------------------------------------------------|
| `service`  | string  | (none)  | Filter by service name                             |
| `severity` | string  | (none)  | Minimum severity: `trace`, `debug`, `info`, `warn`, `error`, `fatal` |
| `search`   | string  | (none)  | Case-insensitive text search in log body           |
| `trace_id` | string  | (none)  | Filter logs by associated trace ID                 |
| `limit`    | integer | `200`   | Maximum number of results                          |

**Example request:**

```bash
curl "http://localhost:4000/api/logs?service=api&severity=error&limit=25"
```

**Example response:**

```json
[
  {
    "record_id": 101,
    "timestamp": "2026-02-22T10:30:00.050Z",
    "service_name": "api",
    "severity": "Error",
    "body": "database connection failed: timeout after 5s",
    "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    "span_id": "1a2b3c4d5e6f7a8b",
    "attributes": [
      ["db.system", "postgresql"]
    ]
  }
]
```

**Log record fields:**

| Field          | Type            | Description                          |
|----------------|-----------------|--------------------------------------|
| `record_id`    | integer         | Internal record ID                   |
| `timestamp`    | string          | ISO 8601 timestamp                   |
| `service_name` | string          | Reporting service name               |
| `severity`     | string          | `Trace`, `Debug`, `Info`, `Warn`, `Error`, or `Fatal` |
| `body`         | string          | Log message body                     |
| `trace_id`     | string or null  | Associated trace ID if present       |
| `span_id`      | string or null  | Associated span ID if present        |
| `attributes`   | [string, string][] | Key-value attribute pairs (max 20)|

---

### GET /api/metrics

Query metric data points from the OTel collector.

**Query parameters:**

| Parameter | Type    | Default | Description                  |
|-----------|---------|---------|------------------------------|
| `name`    | string  | (none)  | Filter by metric name        |
| `service` | string  | (none)  | Filter by service name       |
| `limit`   | integer | `500`   | Maximum number of results    |

**Example request:**

```bash
curl "http://localhost:4000/api/metrics?name=http.server.duration&service=api"
```

**Example response:**

```json
[
  {
    "record_id": 202,
    "timestamp": "2026-02-22T10:30:00.100Z",
    "service_name": "api",
    "metric_name": "http.server.duration",
    "metric_type": "Histogram",
    "value": 142.0,
    "attributes": [
      ["http.method", "GET"],
      ["http.route", "/users"]
    ],
    "unit": "ms"
  }
]
```

**Metric fields:**

| Field          | Type            | Description                             |
|----------------|-----------------|-----------------------------------------|
| `record_id`    | integer         | Internal record ID                      |
| `timestamp`    | string          | ISO 8601 timestamp                      |
| `service_name` | string          | Reporting service name                  |
| `metric_name`  | string          | Metric name (e.g. `http.server.duration`) |
| `metric_type`  | string          | `Gauge`, `Counter`, or `Histogram`      |
| `value`        | number          | Metric value (f64)                      |
| `attributes`   | [string, string][] | Key-value attribute pairs (max 20)   |
| `unit`         | string or null  | Unit of measurement if provided         |

---

### GET /api/status

Get the current system status of the OTel collector.

**Example request:**

```bash
curl "http://localhost:4000/api/status"
```

**Example response:**

```json
{
  "span_count": 1523,
  "log_count": 4210,
  "metric_count": 8934,
  "services": ["api", "web", "worker"],
  "trace_count": 312
}
```

**Status fields:**

| Field          | Type     | Description                                 |
|----------------|----------|---------------------------------------------|
| `span_count`   | integer  | Total number of spans currently stored       |
| `log_count`    | integer  | Total number of log records stored           |
| `metric_count` | integer  | Total number of metric data points stored    |
| `services`     | string[] | Sorted list of service names reporting data  |
| `trace_count`  | integer  | Number of unique trace IDs in the store      |

---

### WebSocket /ws

Real-time telemetry event stream. Connect via WebSocket to receive events
as they are ingested by the OTel collector.

**Example connection:**

```bash
websocat ws://localhost:4000/ws
```

**Event format:**

Each message is a JSON object with a `type` field indicating the event kind
and a `payload` field containing the event data.

**TraceUpdate** -- emitted when a new span is received:

```json
{
  "type": "TraceUpdate",
  "payload": {
    "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    "service": "api",
    "duration_ms": 142,
    "has_error": false
  }
}
```

**LogRecord** -- emitted when a new log is received:

```json
{
  "type": "LogRecord",
  "payload": {
    "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    "severity": "Info",
    "body": "Fetching users from database",
    "service": "api"
  }
}
```

**MetricUpdate** -- emitted when a new metric is received:

```json
{
  "type": "MetricUpdate",
  "payload": {
    "name": "http.server.duration",
    "value": 142.0,
    "service": "api"
  }
}
```

**ServiceStatusChange** -- emitted on service status transitions:

```json
{
  "type": "ServiceStatusChange",
  "payload": {
    "service": "api",
    "status": "running"
  }
}
```

**Connection behavior:**

- The server responds to WebSocket Ping frames with Pong frames.
- If a client falls behind the event stream (broadcast channel lag), missed
  events are silently dropped and the client continues from the latest event.
- The connection closes when the client sends a Close frame, or when the
  server shuts down.
