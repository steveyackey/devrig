import { NodeSDK } from "@opentelemetry/sdk-node";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-http";
import { PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import {
  trace,
  context,
  propagation,
  SpanKind,
  SpanStatusCode,
  type Span,
  type Context,
} from "@opentelemetry/api";

const endpoint = process.env.OTEL_EXPORTER_OTLP_ENDPOINT;

const sdk = new NodeSDK({
  serviceName: process.env.OTEL_SERVICE_NAME,
  traceExporter: new OTLPTraceExporter({ url: `${endpoint}/v1/traces` }),
  metricReader: new PeriodicExportingMetricReader({
    exporter: new OTLPMetricExporter({ url: `${endpoint}/v1/metrics` }),
    exportIntervalMillis: 10000,
  }),
});
sdk.start();

export const tracer = trace.getTracer("demo");

export { SpanKind, SpanStatusCode, context, propagation, trace };

/** Extract W3C trace context from incoming request headers. */
export function extractContext(headers: Headers): Context {
  const carrier: Record<string, string> = {};
  headers.forEach((value, key) => {
    carrier[key] = value;
  });
  return propagation.extract(context.active(), carrier);
}

/** Inject W3C trace context into outgoing request headers. */
export function injectHeaders(
  ctx?: Context,
): Record<string, string> {
  const headers: Record<string, string> = {};
  propagation.inject(ctx ?? context.active(), headers);
  return headers;
}

/** Run an async function inside a new span. */
export async function withSpan<T>(
  name: string,
  kind: SpanKind,
  fn: (span: Span) => Promise<T>,
  parentCtx?: Context,
): Promise<T> {
  const ctx = parentCtx ?? context.active();
  return tracer.startActiveSpan(name, { kind }, ctx, async (span) => {
    try {
      const result = await fn(span);
      span.setStatus({ code: SpanStatusCode.OK });
      return result;
    } catch (err) {
      span.setStatus({
        code: SpanStatusCode.ERROR,
        message: err instanceof Error ? err.message : String(err),
      });
      throw err;
    } finally {
      span.end();
    }
  });
}
