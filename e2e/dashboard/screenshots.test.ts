import { describe, test, beforeAll, afterAll } from 'bun:test';
import { sharedBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';
import { execSync } from 'child_process';
import * as path from 'path';

const SCREENSHOT_DIR = '../docs/images';
const GENERATOR_DIR = path.resolve(__dirname, '../../examples/telemetry-generator');

function runGenerator() {
  const otlpPort = process.env.OTLP_HTTP_PORT ?? '4318';
  execSync(
    `bun run src/index.ts --otlp-url http://localhost:${otlpPort} --duration 30 --traces 20 --logs 60`,
    {
      cwd: GENERATOR_DIR,
      stdio: 'pipe',
      timeout: 30_000,
    },
  );
}

describe.skipIf(!process.env.SCREENSHOTS)('Screenshot regeneration', () => {
  let browser: Browser;
  let page: Page;

  beforeAll(async () => {
    browser = await sharedBrowser();
    page = await newPage(browser);
  });

  afterAll(async () => {
    await page.context().close();
  });

  test('seed telemetry data via generator', async () => {
    runGenerator();
    // Allow data ingestion
    await new Promise((r) => setTimeout(r, 1500));
  }, 45_000);

  test('screenshot: traces view', async () => {
    await page.goto('/#/traces');
    await page.locator('[data-testid="traces-view"]').waitFor();
    await page.locator('[data-testid="trace-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-traces.png`, fullPage: true });
  }, 20_000);

  test('screenshot: trace detail (waterfall)', async () => {
    await page.goto('/#/traces');
    await page.locator('[data-testid="traces-view"]').waitFor();
    const firstRow = page.locator('[data-testid="trace-row"]').first();
    await firstRow.waitFor({ timeout: 10000 }).catch(() => {});

    if (await firstRow.isVisible()) {
      await firstRow.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-trace-detail.png`, fullPage: true });
    }
  }, 20_000);

  test('screenshot: logs view', async () => {
    await page.goto('/#/logs');
    await page.locator('[data-testid="logs-view"]').waitFor();
    await page.locator('[data-testid="log-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-logs.png`, fullPage: true });
  }, 20_000);

  test('screenshot: metrics view', async () => {
    await page.goto('/#/metrics');
    await page.locator('[data-testid="metrics-view"]').waitFor();
    // Wait for metric cards or rows to appear
    await page.locator('[data-testid="metric-card"], [data-testid="metric-row"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-metrics.png`, fullPage: true });
  }, 20_000);

  test('screenshot: status view', async () => {
    await page.goto('/#/status');
    await page.locator('[data-testid="status-view"]').waitFor();
    await page.locator('[data-testid="stat-card"]').first().waitFor({ timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SCREENSHOT_DIR}/dashboard-status.png`, fullPage: true });
  }, 20_000);
});
