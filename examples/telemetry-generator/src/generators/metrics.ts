function createRng(seed: number) {
  let s = seed;
  return () => {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    return (s >>> 0) / 4294967296;
  };
}

interface MetricDef {
  name: string;
  unit: string;
  type: "gauge" | "counter" | "histogram";
  services: string[];
  generate: (rng: () => number, step: number, total: number) => number;
}

const METRIC_DEFS: MetricDef[] = [
  {
    name: "http.server.request.duration",
    unit: "ms",
    type: "histogram",
    services: ["api-server", "web-frontend"],
    generate: (rng, _step, _total) => {
      // Realistic latency distribution: mostly fast, occasional slow
      const base = 15 + rng() * 40;
      const spike = rng() < 0.1 ? rng() * 500 : 0;
      return Math.round((base + spike) * 100) / 100;
    },
  },
  {
    name: "http.server.active_requests",
    unit: "{requests}",
    type: "gauge",
    services: ["api-server", "web-frontend"],
    generate: (rng, step, total) => {
      // Fluctuating gauge with daily-like pattern
      const phase = (step / total) * Math.PI * 2;
      const base = 8 + Math.sin(phase) * 5;
      const noise = (rng() - 0.5) * 4;
      return Math.max(0, Math.round(base + noise));
    },
  },
  {
    name: "http.server.request.total",
    unit: "{requests}",
    type: "counter",
    services: ["api-server", "web-frontend"],
    generate: (rng, step, _total) => {
      // Monotonically increasing counter
      const baseRate = 50 + rng() * 30;
      return Math.round(baseRate * (step + 1));
    },
  },
  {
    name: "process.runtime.memory",
    unit: "By",
    type: "gauge",
    services: ["api-server", "web-frontend", "worker"],
    generate: (rng, step, total) => {
      // Memory usage with gradual increase and GC drops
      const base = 50_000_000 + step * 100_000;
      const gcDrop = step > 0 && step % 12 === 0 ? -20_000_000 : 0;
      const noise = (rng() - 0.5) * 5_000_000;
      return Math.max(10_000_000, Math.round(base + gcDrop + noise));
    },
  },
  {
    name: "db.client.connections.active",
    unit: "{connections}",
    type: "gauge",
    services: ["api-server", "worker"],
    generate: (rng, step, total) => {
      const phase = (step / total) * Math.PI * 4;
      const base = 5 + Math.sin(phase) * 3;
      const noise = (rng() - 0.5) * 2;
      return Math.max(1, Math.round(base + noise));
    },
  },
];

export interface MetricPayload {
  resourceMetrics: Array<{
    resource: { attributes: Array<{ key: string; value: { stringValue: string } }> };
    scopeMetrics: Array<{
      scope: { name: string };
      metrics: Array<Record<string, unknown>>;
    }>;
  }>;
}

export function generateMetrics(
  durationSec: number = 30,
  intervalSec: number = 5,
  seed: number = 77,
): MetricPayload {
  const rng = createRng(seed);
  const steps = Math.ceil(durationSec / intervalSec);

  // Collect metrics grouped by service
  const byService = new Map<string, Array<Record<string, unknown>>>();

  for (const def of METRIC_DEFS) {
    for (const service of def.services) {
      const dataPoints: Array<Record<string, unknown>> = [];

      for (let step = 0; step < steps; step++) {
        const timeMs = Date.now() - (steps - step - 1) * intervalSec * 1000;
        const timeNs = (BigInt(timeMs) * 1_000_000n).toString();
        const startTimeNs = (BigInt(timeMs - intervalSec * 1000) * 1_000_000n).toString();
        const value = def.generate(rng, step, steps);

        if (def.type === "gauge") {
          dataPoints.push({
            timeUnixNano: timeNs,
            asDouble: value,
            attributes: [],
          });
        } else if (def.type === "counter") {
          dataPoints.push({
            startTimeUnixNano: startTimeNs,
            timeUnixNano: timeNs,
            asInt: String(Math.round(value)),
            attributes: [],
          });
        } else if (def.type === "histogram") {
          // Emit as a histogram sum for simplicity
          dataPoints.push({
            startTimeUnixNano: startTimeNs,
            timeUnixNano: timeNs,
            count: String(Math.round(5 + rng() * 20)),
            sum: value,
            bucketCounts: [
              String(Math.round(rng() * 5)),
              String(Math.round(rng() * 10)),
              String(Math.round(rng() * 8)),
              String(Math.round(rng() * 4)),
              String(Math.round(rng() * 2)),
              String(Math.round(rng() * 1)),
            ],
            explicitBounds: [10, 25, 50, 100, 250],
            attributes: [],
          });
        }
      }

      // Build OTLP metric object
      let metric: Record<string, unknown>;
      if (def.type === "gauge") {
        metric = {
          name: def.name,
          unit: def.unit,
          gauge: { dataPoints },
        };
      } else if (def.type === "counter") {
        metric = {
          name: def.name,
          unit: def.unit,
          sum: {
            dataPoints,
            aggregationTemporality: 2,
            isMonotonic: true,
          },
        };
      } else {
        metric = {
          name: def.name,
          unit: def.unit,
          histogram: {
            dataPoints,
            aggregationTemporality: 2,
          },
        };
      }

      const existing = byService.get(service) ?? [];
      existing.push(metric);
      byService.set(service, existing);
    }
  }

  return {
    resourceMetrics: Array.from(byService.entries()).map(([service, metrics]) => ({
      resource: {
        attributes: [{ key: "service.name", value: { stringValue: service } }],
      },
      scopeMetrics: [
        {
          scope: { name: "devrig-telemetry-generator" },
          metrics,
        },
      ],
    })),
  };
}
