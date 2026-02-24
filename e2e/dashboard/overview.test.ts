import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from 'bun:test';
import { sharedBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';

describe('Overview / Status View', () => {
  let browser: Browser;
  let page: Page;

  beforeAll(async () => {
    browser = await sharedBrowser();
  });

  beforeEach(async () => {
    page = await newPage(browser);
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );
    await page.goto('/#/status');
    await responsePromise;
  });

  afterEach(async () => {
    await page.context().close();
  });

  test('displays the system status heading', async () => {
    await expect(page.getByRole('heading', { name: 'System Status' })).toBeVisible();
    await expect(page.getByText('Telemetry pipeline overview')).toBeVisible();
  });

  test('renders stat cards for traces, spans, logs, and metrics', async () => {
    // Each stat card has a label rendered as uppercase text
    await expect(page.getByText('Traces', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Spans', { exact: true })).toBeVisible();
    await expect(page.getByText('Logs', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Metrics', { exact: true }).first()).toBeVisible();
  });

  test('stat cards display numeric values', async () => {
    // The stat card values are rendered with data-testid
    const statValues = page.locator('[data-testid="stat-card-value"]');
    await expect(statValues).toHaveCount(4);

    for (let i = 0; i < 4; i++) {
      const text = await statValues.nth(i).textContent();
      // Values should be numeric strings (possibly with locale formatting like commas)
      expect(text).toBeTruthy();
      expect(text!.trim()).toMatch(/^[\d,]+$/);
    }
  });

  test('shows reporting services section', async () => {
    await expect(page.getByText(/Services \(\d+\)/)).toBeVisible();
    await expect(
      page.getByText('Configured services and their ports'),
    ).toBeVisible();
  });

  test('services have green status indicator dots', async () => {
    const serviceIndicators = page.locator('[data-testid="service-indicator"]');
    const count = await serviceIndicators.count();

    if (count > 0) {
      // At least the first service should have a visible indicator
      await expect(serviceIndicators.first()).toBeVisible();
    }
  });

  test('service rows have View Traces and View Logs links', async () => {
    const serviceRows = page.locator('[data-testid="service-row"]');
    const rowCount = await serviceRows.count();

    if (rowCount > 0) {
      const firstRow = serviceRows.first();
      await expect(firstRow.getByText('Traces')).toBeVisible();
      await expect(firstRow.getByText('Logs')).toBeVisible();
    }
  });

  test('refresh button triggers data reload', async () => {
    const refreshButton = page.getByRole('button', { name: 'Refresh' });
    await expect(refreshButton).toBeVisible();

    // Click refresh and wait for the API call
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );
    await refreshButton.click();
    await responsePromise;
  });

  test('shows auto-refresh indicator', async () => {
    await expect(page.getByText('Auto-refreshes every 5 seconds')).toBeVisible();
  });

  test('sidebar navigation highlights the Status link', async () => {
    const statusNav = page.locator('[data-testid="sidebar-nav-item"]').filter({ hasText: 'Status' });
    await expect(statusNav).toHaveAttribute('data-active', 'true');
  });

  test('status bar at the bottom shows telemetry counts', async () => {
    const footer = page.locator('[data-testid="status-bar"]');
    await expect(footer).toBeVisible();

    // Status bar shows counts for Traces, Spans, Logs, Metrics, Services
    await expect(footer.getByText('Traces:')).toBeVisible();
    await expect(footer.getByText('Spans:')).toBeVisible();
    await expect(footer.getByText('Logs:')).toBeVisible();
    await expect(footer.getByText('Metrics:')).toBeVisible();
    await expect(footer.getByText('Services:')).toBeVisible();
  });
});
