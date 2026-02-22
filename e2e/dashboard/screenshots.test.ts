import { test, expect } from '@playwright/test';
import { execSync } from 'child_process';

const OTLP_HTTP = process.env.OTLP_HTTP_PORT ?? '4318';
const OTLP_URL = `http://localhost:${OTLP_HTTP}`;
const SCREENSHOT_DIR = '../docs/images';

// Generate a random hex string of given byte length
function randomHex(bytes: number): string {
  return Array.from({ length: bytes }, () =>
    Math.floor(Math.random() * 256).toString(16).padStart(2, '0'),
  ).join('');
}

function nowNs(): string {
  return (BigInt(Date.now()) * 1_000_000n).toString();
}

function seedOtlpData() {
  const traceId = randomHex(16);
  const rootSpanId = randomHex(8);
  const childSpanId = randomHex(8);
  const now = nowNs();
  const startNs = (BigInt(Date.now() - 150) * 1_000_000n).toString();

  const tracePayload = {
    resourceSpans: [
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'web-frontend' } }] },
        scopeSpans: [
          {
            scope: { name: 'screenshot-seed' },
            spans: [
              {
                traceId,
                spanId: rootSpanId,
                name: 'GET /api/products',
                kind: 2,
                startTimeUnixNano: startNs,
                endTimeUnixNano: now,
                status: { code: 1 },
                attributes: [
                  { key: 'http.method', value: { stringValue: 'GET' } },
                  { key: 'http.route', value: { stringValue: '/api/products' } },
                  { key: 'http.status_code', value: { intValue: '200' } },
                ],
              },
              {
                traceId,
                spanId: childSpanId,
                parentSpanId: rootSpanId,
                name: 'SELECT products',
                kind: 3,
                startTimeUnixNano: (BigInt(Date.now() - 120) * 1_000_000n).toString(),
                endTimeUnixNano: (BigInt(Date.now() - 30) * 1_000_000n).toString(),
                status: { code: 1 },
                attributes: [
                  { key: 'db.system', value: { stringValue: 'postgresql' } },
                  { key: 'db.statement', value: { stringValue: 'SELECT * FROM products' } },
                ],
              },
            ],
          },
        ],
      },
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'api-server' } }] },
        scopeSpans: [
          {
            scope: { name: 'screenshot-seed' },
            spans: [
              {
                traceId,
                spanId: randomHex(8),
                parentSpanId: rootSpanId,
                name: 'ProductService.list',
                kind: 1,
                startTimeUnixNano: (BigInt(Date.now() - 140) * 1_000_000n).toString(),
                endTimeUnixNano: (BigInt(Date.now() - 10) * 1_000_000n).toString(),
                status: { code: 1 },
                attributes: [
                  { key: 'rpc.method', value: { stringValue: 'list' } },
                ],
              },
            ],
          },
        ],
      },
    ],
  };

  // Additional traces for a fuller list
  for (let i = 0; i < 5; i++) {
    const tid = randomHex(16);
    const sid = randomHex(8);
    const hasError = i === 2;
    const svcName = i % 2 === 0 ? 'web-frontend' : 'api-server';
    const ops = ['POST /api/orders', 'GET /api/users', 'DELETE /api/cache', 'PUT /api/settings', 'GET /healthz'];
    tracePayload.resourceSpans.push({
      resource: { attributes: [{ key: 'service.name', value: { stringValue: svcName } }] },
      scopeSpans: [
        {
          scope: { name: 'screenshot-seed' },
          spans: [
            {
              traceId: tid,
              spanId: sid,
              name: ops[i],
              kind: 2,
              startTimeUnixNano: (BigInt(Date.now() - 200 - i * 100) * 1_000_000n).toString(),
              endTimeUnixNano: (BigInt(Date.now() - i * 50) * 1_000_000n).toString(),
              status: { code: hasError ? 2 : 1 },
              attributes: [
                { key: 'http.method', value: { stringValue: ops[i].split(' ')[0] } },
                { key: 'http.status_code', value: { intValue: hasError ? '500' : '200' } },
              ],
            } as any,
          ],
        },
      ],
    });
  }

  const logPayload = {
    resourceLogs: [
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'api-server' } }] },
        scopeLogs: [
          {
            scope: { name: 'screenshot-seed' },
            logRecords: [
              { timeUnixNano: now, severityNumber: 9, severityText: 'Info', body: { stringValue: 'Server started on port 3000' }, traceId, spanId: rootSpanId },
              { timeUnixNano: now, severityNumber: 9, severityText: 'Info', body: { stringValue: 'Connected to database' } },
              { timeUnixNano: now, severityNumber: 13, severityText: 'Warn', body: { stringValue: 'Slow query detected: 250ms' }, traceId, spanId: childSpanId },
              { timeUnixNano: now, severityNumber: 17, severityText: 'Error', body: { stringValue: 'Connection pool exhausted, retrying...' } },
              { timeUnixNano: now, severityNumber: 5, severityText: 'Debug', body: { stringValue: 'Cache miss for key: products:all' } },
            ],
          },
        ],
      },
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'web-frontend' } }] },
        scopeLogs: [
          {
            scope: { name: 'screenshot-seed' },
            logRecords: [
              { timeUnixNano: now, severityNumber: 9, severityText: 'Info', body: { stringValue: 'Rendering product list page' }, traceId, spanId: rootSpanId },
              { timeUnixNano: now, severityNumber: 9, severityText: 'Info', body: { stringValue: 'Hydration complete in 45ms' } },
            ],
          },
        ],
      },
    ],
  };

  const metricPayload = {
    resourceMetrics: [
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'api-server' } }] },
        scopeMetrics: [
          {
            scope: { name: 'screenshot-seed' },
            metrics: [
              {
                name: 'http.server.request.duration',
                unit: 'ms',
                histogram: {
                  dataPoints: [
                    {
                      startTimeUnixNano: startNs,
                      timeUnixNano: now,
                      count: '42',
                      sum: 1250.5,
                      bucketCounts: ['5', '10', '15', '8', '3', '1'],
                      explicitBounds: [10, 25, 50, 100, 250],
                      attributes: [{ key: 'http.route', value: { stringValue: '/api/products' } }],
                    },
                  ],
                  aggregationTemporality: 2,
                },
              },
              {
                name: 'http.server.active_requests',
                unit: '{requests}',
                gauge: {
                  dataPoints: [
                    { timeUnixNano: now, asInt: '12', attributes: [] },
                  ],
                },
              },
              {
                name: 'http.server.request.total',
                unit: '{requests}',
                sum: {
                  dataPoints: [
                    { startTimeUnixNano: startNs, timeUnixNano: now, asInt: '1847', attributes: [] },
                  ],
                  aggregationTemporality: 2,
                  isMonotonic: true,
                },
              },
            ],
          },
        ],
      },
      {
        resource: { attributes: [{ key: 'service.name', value: { stringValue: 'web-frontend' } }] },
        scopeMetrics: [
          {
            scope: { name: 'screenshot-seed' },
            metrics: [
              {
                name: 'browser.page.load_time',
                unit: 'ms',
                gauge: {
                  dataPoints: [
                    { timeUnixNano: now, asDouble: 342.7, attributes: [] },
                  ],
                },
              },
            ],
          },
        ],
      },
    ],
  };

  // Send all telemetry via curl
  const curlOpts = `-s -f -H "Content-Type: application/json"`;
  execSync(`curl ${curlOpts} -d '${JSON.stringify(tracePayload)}' ${OTLP_URL}/v1/traces`, { stdio: 'pipe' });
  execSync(`curl ${curlOpts} -d '${JSON.stringify(logPayload)}' ${OTLP_URL}/v1/logs`, { stdio: 'pipe' });
  execSync(`curl ${curlOpts} -d '${JSON.stringify(metricPayload)}' ${OTLP_URL}/v1/metrics`, { stdio: 'pipe' });
}

test.describe('Screenshot regeneration @screenshots', () => {
  test.describe.configure({ mode: 'serial' });

  let seededTraceId: string;

  test('seed telemetry data', async () => {
    // Capture a trace ID before seeding so we can navigate to its detail
    const traceId = randomHex(16);
    seededTraceId = traceId;
    seedOtlpData();
    // Brief pause for data to be ingested
    await new Promise((r) => setTimeout(r, 1000));
  });

  test('screenshot: traces view', async ({ page }) => {
    await page.goto('/#/traces');
    await page.locator('[data-testid="traces-view"]').waitFor();
    await page.locator('[data-testid="trace-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-traces.png`, fullPage: true });
  });

  test('screenshot: trace detail (waterfall)', async ({ page }) => {
    await page.goto('/#/traces');
    await page.locator('[data-testid="traces-view"]').waitFor();
    const firstRow = page.locator('[data-testid="trace-row"]').first();
    await firstRow.waitFor({ timeout: 10000 }).catch(() => {});

    if (await firstRow.isVisible()) {
      await firstRow.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-trace-detail.png`, fullPage: true });
    }
  });

  test('screenshot: logs view', async ({ page }) => {
    await page.goto('/#/logs');
    await page.locator('[data-testid="logs-view"]').waitFor();
    await page.locator('[data-testid="log-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-logs.png`, fullPage: true });
  });

  test('screenshot: metrics view', async ({ page }) => {
    await page.goto('/#/metrics');
    await page.locator('[data-testid="metrics-view"]').waitFor();
    await page.locator('[data-testid="metric-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-metrics.png`, fullPage: true });
  });

  test('screenshot: status view', async ({ page }) => {
    await page.goto('/#/status');
    await page.locator('[data-testid="status-view"]').waitFor();
    await page.locator('[data-testid="stat-card"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-status.png`, fullPage: true });
  });
});
