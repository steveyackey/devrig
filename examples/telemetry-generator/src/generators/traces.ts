// Seeded PRNG for reproducible data (xorshift32)
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
    Math.floor(rng() * 256)
      .toString(16)
      .padStart(2, "0"),
  ).join("");
}

interface SpanDef {
  service: string;
  name: string;
  kind: number;
  minMs: number;
  maxMs: number;
  errorRate: number;
  children?: SpanDef[];
  attributes?: Array<{ key: string; value: { stringValue: string } }>;
}

const TRACE_TEMPLATES: SpanDef[] = [
  {
    service: "web-frontend",
    name: "GET /api/products",
    kind: 2,
    minMs: 80,
    maxMs: 350,
    errorRate: 0.05,
    attributes: [
      { key: "http.method", value: { stringValue: "GET" } },
      { key: "http.route", value: { stringValue: "/api/products" } },
    ],
    children: [
      {
        service: "api-server",
        name: "ProductService.list",
        kind: 2,
        minMs: 40,
        maxMs: 200,
        errorRate: 0.03,
        attributes: [{ key: "rpc.method", value: { stringValue: "list" } }],
        children: [
          {
            service: "api-server",
            name: "SELECT products",
            kind: 3,
            minMs: 5,
            maxMs: 80,
            errorRate: 0.02,
            attributes: [
              { key: "db.system", value: { stringValue: "postgresql" } },
              { key: "db.statement", value: { stringValue: "SELECT * FROM products WHERE active = true" } },
            ],
          },
          {
            service: "api-server",
            name: "redis.get products:cache",
            kind: 3,
            minMs: 1,
            maxMs: 10,
            errorRate: 0.01,
            attributes: [
              { key: "db.system", value: { stringValue: "redis" } },
              { key: "db.operation", value: { stringValue: "GET" } },
            ],
          },
        ],
      },
    ],
  },
  {
    service: "web-frontend",
    name: "POST /api/orders",
    kind: 2,
    minMs: 150,
    maxMs: 800,
    errorRate: 0.1,
    attributes: [
      { key: "http.method", value: { stringValue: "POST" } },
      { key: "http.route", value: { stringValue: "/api/orders" } },
    ],
    children: [
      {
        service: "api-server",
        name: "OrderService.create",
        kind: 2,
        minMs: 100,
        maxMs: 500,
        errorRate: 0.08,
        children: [
          {
            service: "api-server",
            name: "INSERT INTO orders",
            kind: 3,
            minMs: 10,
            maxMs: 100,
            errorRate: 0.03,
            attributes: [
              { key: "db.system", value: { stringValue: "postgresql" } },
              { key: "db.statement", value: { stringValue: "INSERT INTO orders (user_id, total) VALUES ($1, $2)" } },
            ],
          },
          {
            service: "worker",
            name: "NotificationWorker.send",
            kind: 4,
            minMs: 20,
            maxMs: 200,
            errorRate: 0.15,
            attributes: [
              { key: "messaging.system", value: { stringValue: "rabbitmq" } },
              { key: "messaging.operation", value: { stringValue: "publish" } },
            ],
          },
        ],
      },
    ],
  },
  {
    service: "api-server",
    name: "GET /api/users/{id}",
    kind: 2,
    minMs: 30,
    maxMs: 150,
    errorRate: 0.07,
    attributes: [
      { key: "http.method", value: { stringValue: "GET" } },
      { key: "http.route", value: { stringValue: "/api/users/{id}" } },
    ],
    children: [
      {
        service: "api-server",
        name: "SELECT users WHERE id = $1",
        kind: 3,
        minMs: 3,
        maxMs: 40,
        errorRate: 0.01,
        attributes: [
          { key: "db.system", value: { stringValue: "postgresql" } },
        ],
      },
    ],
  },
  {
    service: "worker",
    name: "CronJob.cleanup",
    kind: 1,
    minMs: 500,
    maxMs: 3000,
    errorRate: 0.12,
    attributes: [
      { key: "job.type", value: { stringValue: "cleanup" } },
    ],
    children: [
      {
        service: "worker",
        name: "DELETE expired_sessions",
        kind: 3,
        minMs: 50,
        maxMs: 500,
        errorRate: 0.05,
        attributes: [
          { key: "db.system", value: { stringValue: "postgresql" } },
        ],
      },
    ],
  },
  {
    service: "web-frontend",
    name: "GET /healthz",
    kind: 2,
    minMs: 1,
    maxMs: 10,
    errorRate: 0.0,
    attributes: [
      { key: "http.method", value: { stringValue: "GET" } },
      { key: "http.route", value: { stringValue: "/healthz" } },
      { key: "http.status_code", value: { stringValue: "200" } },
    ],
  },
];

function nowNs(): string {
  return (BigInt(Date.now()) * 1_000_000n).toString();
}

function msToNs(ms: number): bigint {
  return BigInt(Math.round(ms)) * 1_000_000n;
}

interface GeneratedSpan {
  traceId: string;
  spanId: string;
  parentSpanId?: string;
  name: string;
  kind: number;
  startTimeUnixNano: string;
  endTimeUnixNano: string;
  status: { code: number; message?: string };
  attributes: Array<{ key: string; value: { stringValue: string } }>;
  service: string; // for grouping, not part of OTLP span
}

function generateSpans(
  def: SpanDef,
  traceId: string,
  parentSpanId: string | undefined,
  baseTimeMs: number,
  rng: () => number,
): GeneratedSpan[] {
  const spanId = randomHex(8, rng);
  const durationMs = def.minMs + rng() * (def.maxMs - def.minMs);
  const startMs = baseTimeMs;
  const endMs = startMs + durationMs;
  const hasError = rng() < def.errorRate;

  const span: GeneratedSpan = {
    traceId,
    spanId,
    parentSpanId,
    name: def.name,
    kind: def.kind,
    startTimeUnixNano: msToNs(startMs).toString(),
    endTimeUnixNano: msToNs(endMs).toString(),
    status: { code: hasError ? 2 : 1, ...(hasError ? { message: "Internal error" } : {}) },
    attributes: [
      ...(def.attributes ?? []),
      ...(hasError ? [{ key: "http.status_code", value: { stringValue: "500" } }] : []),
    ],
    service: def.service,
  };

  const spans = [span];

  if (def.children) {
    let childStart = startMs + durationMs * 0.1;
    for (const child of def.children) {
      const childSpans = generateSpans(child, traceId, spanId, childStart, rng);
      spans.push(...childSpans);
      childStart += (durationMs * 0.3) / def.children.length;
    }
  }

  return spans;
}

export interface TracePayload {
  resourceSpans: Array<{
    resource: { attributes: Array<{ key: string; value: { stringValue: string } }> };
    scopeSpans: Array<{
      scope: { name: string };
      spans: Array<Omit<GeneratedSpan, "service">>;
    }>;
  }>;
}

export function generateTraces(count: number, seed: number = 42): TracePayload {
  const rng = createRng(seed);
  const allSpans: GeneratedSpan[] = [];

  for (let i = 0; i < count; i++) {
    const template = TRACE_TEMPLATES[Math.floor(rng() * TRACE_TEMPLATES.length)];
    const traceId = randomHex(16, rng);
    const baseTimeMs = Date.now() - rng() * 30_000; // within last 30s
    const spans = generateSpans(template, traceId, undefined, baseTimeMs, rng);
    allSpans.push(...spans);
  }

  // Group by service
  const byService = new Map<string, GeneratedSpan[]>();
  for (const span of allSpans) {
    const existing = byService.get(span.service) ?? [];
    existing.push(span);
    byService.set(span.service, existing);
  }

  const resourceSpans = Array.from(byService.entries()).map(([service, spans]) => ({
    resource: {
      attributes: [{ key: "service.name", value: { stringValue: service } }],
    },
    scopeSpans: [
      {
        scope: { name: "devrig-telemetry-generator" },
        spans: spans.map(({ service: _svc, ...rest }) => rest),
      },
    ],
  }));

  return { resourceSpans };
}
