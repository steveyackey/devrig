function createRng(seed: number) {
  let s = seed;
  return () => {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    return (s >>> 0) / 4294967296;
  };
}

function randomHex(bytes: number, rng: () => number): string {
  return Array.from({ length: bytes }, () =>
    Math.floor(rng() * 256).toString(16).padStart(2, "0"),
  ).join("");
}

interface LogTemplate {
  service: string;
  severity: number;
  severityText: string;
  messages: string[];
}

const LOG_TEMPLATES: LogTemplate[] = [
  { service: "api-server", severity: 9, severityText: "Info", messages: [
    "Server started on port 3000",
    "Connected to database",
    "Request processed successfully",
    "Cache hit for key: products:all",
    "Health check passed",
    "New connection from 10.0.0.1",
    "Session created for user_123",
  ]},
  { service: "api-server", severity: 13, severityText: "Warn", messages: [
    "Slow query detected: 250ms",
    "Rate limit approaching for client 10.0.0.5",
    "Deprecated API endpoint called: /v1/legacy",
    "Connection pool at 80% capacity",
    "Retry attempt 2/3 for external API call",
  ]},
  { service: "api-server", severity: 17, severityText: "Error", messages: [
    "Connection pool exhausted, retrying...",
    "Failed to parse request body: invalid JSON",
    "Database query timeout after 5000ms",
    "Authentication failed for token: expired",
    "External service unavailable: payment-gateway",
  ]},
  { service: "api-server", severity: 5, severityText: "Debug", messages: [
    "Cache miss for key: products:all",
    "Query plan: sequential scan on orders",
    "Middleware chain: [auth, cors, logging, handler]",
    "Response serialized in 2ms",
  ]},
  { service: "web-frontend", severity: 9, severityText: "Info", messages: [
    "Rendering product list page",
    "Hydration complete in 45ms",
    "Route changed: / -> /products",
    "Asset bundle loaded: 245KB",
    "Service worker registered",
  ]},
  { service: "web-frontend", severity: 13, severityText: "Warn", messages: [
    "Large DOM size detected: 1500 nodes",
    "Image lazy-load fallback activated",
    "WebSocket reconnecting...",
  ]},
  { service: "web-frontend", severity: 17, severityText: "Error", messages: [
    "Failed to fetch /api/products: NetworkError",
    "Unhandled promise rejection in ProductList",
    "WebSocket connection lost",
  ]},
  { service: "worker", severity: 9, severityText: "Info", messages: [
    "Job started: email-notifications",
    "Processed 42 items in batch",
    "Cleanup completed: removed 15 expired sessions",
    "Queue depth: 3 pending jobs",
  ]},
  { service: "worker", severity: 17, severityText: "Error", messages: [
    "Job failed: email-notifications - SMTP timeout",
    "Dead letter queue overflow: 100 messages",
  ]},
  { service: "worker", severity: 1, severityText: "Trace", messages: [
    "Entering job handler: cleanup",
    "Exiting job handler: cleanup (ok)",
  ]},
];

export interface LogPayload {
  resourceLogs: Array<{
    resource: { attributes: Array<{ key: string; value: { stringValue: string } }> };
    scopeLogs: Array<{
      scope: { name: string };
      logRecords: Array<{
        timeUnixNano: string;
        severityNumber: number;
        severityText: string;
        body: { stringValue: string };
        traceId?: string;
        spanId?: string;
      }>;
    }>;
  }>;
}

export function generateLogs(count: number, seed: number = 99): LogPayload {
  const rng = createRng(seed);
  const records: Array<{
    service: string;
    record: {
      timeUnixNano: string;
      severityNumber: number;
      severityText: string;
      body: { stringValue: string };
      traceId?: string;
      spanId?: string;
    };
  }> = [];

  // Severity distribution: 60% Info, 15% Debug, 10% Warn, 10% Error, 5% Trace
  const weights = [
    { min: 0, max: 0.60, severity: 9, text: "Info" },
    { min: 0.60, max: 0.75, severity: 5, text: "Debug" },
    { min: 0.75, max: 0.85, severity: 13, text: "Warn" },
    { min: 0.85, max: 0.95, severity: 17, text: "Error" },
    { min: 0.95, max: 1.0, severity: 1, text: "Trace" },
  ];

  for (let i = 0; i < count; i++) {
    const r = rng();
    const w = weights.find((w) => r >= w.min && r < w.max) ?? weights[0];

    // Find matching templates
    const matching = LOG_TEMPLATES.filter(
      (t) => t.severity === w.severity,
    );
    if (matching.length === 0) continue;

    const template = matching[Math.floor(rng() * matching.length)];
    const message = template.messages[Math.floor(rng() * template.messages.length)];

    const timeMs = Date.now() - rng() * 30_000;
    const timeNs = (BigInt(Math.round(timeMs)) * 1_000_000n).toString();

    // 30% chance of being correlated with a trace
    const hasTrace = rng() < 0.3;

    records.push({
      service: template.service,
      record: {
        timeUnixNano: timeNs,
        severityNumber: w.severity,
        severityText: w.text,
        body: { stringValue: message },
        ...(hasTrace ? { traceId: randomHex(16, rng), spanId: randomHex(8, rng) } : {}),
      },
    });
  }

  // Group by service
  const byService = new Map<string, typeof records>();
  for (const r of records) {
    const existing = byService.get(r.service) ?? [];
    existing.push(r);
    byService.set(r.service, existing);
  }

  return {
    resourceLogs: Array.from(byService.entries()).map(([service, recs]) => ({
      resource: {
        attributes: [{ key: "service.name", value: { stringValue: service } }],
      },
      scopeLogs: [
        {
          scope: { name: "devrig-telemetry-generator" },
          logRecords: recs.map((r) => r.record),
        },
      ],
    })),
  };
}
