import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from 'bun:test';
import { sharedBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';

describe('Metrics View', () => {
  let browser: Browser;
  let page: Page;

  beforeAll(async () => {
    browser = await sharedBrowser();
  });

  beforeEach(async () => {
    page = await newPage(browser);
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
    await page.goto('/#/metrics');
    await responsePromise;
  });

  afterEach(async () => {
    await page.context().close();
  });

  test('displays the metrics heading', async () => {
    await expect(page.getByRole('heading', { name: 'Metrics' })).toBeVisible();
    await expect(page.getByText('Telemetry metric data points')).toBeVisible();
  });

  test('renders the filter bar with metric name and service filter', async () => {
    // Metric name input
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await expect(nameInput).toBeVisible();

    // Service dropdown
    const serviceSelect = page.locator('select').filter({ hasText: 'All Services' });
    await expect(serviceSelect).toBeVisible();

    // Search and Clear buttons
    await expect(page.getByRole('button', { name: 'Search' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Clear' })).toBeVisible();
  });

  test('metrics table has correct column headers', async () => {
    const headers = page.locator('thead th');
    const headerTexts = await headers.allTextContents();
    const normalized = headerTexts.map((t) => t.trim().toLowerCase());

    expect(normalized).toContain('time');
    expect(normalized).toContain('service');
    expect(normalized).toContain('metric name');
    expect(normalized).toContain('type');
    expect(normalized).toContain('value');
    expect(normalized).toContain('unit');
  });

  test('metric rows render with name, type badge, and value', async () => {
    const rows = page.locator('[data-testid="metric-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const firstRow = rows.first();

      // Metric name
      const metricName = firstRow.locator('[data-testid="metric-name"]');
      await expect(metricName).toBeVisible();

      // Type badge (Counter, Gauge, or Histogram)
      const typeBadge = firstRow.locator('[data-testid="metric-type-badge"]');
      await expect(typeBadge).toBeVisible();
      const badgeText = await typeBadge.textContent();
      expect(['Counter', 'Gauge', 'Histogram']).toContain(badgeText!.trim());

      // Numeric value
      const valueCell = firstRow.locator('[data-testid="metric-value"]');
      await expect(valueCell).toBeVisible();
    }
  });

  test('type badges have correct color coding', async () => {
    const rows = page.locator('[data-testid="metric-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const typeBadges = page.locator('[data-testid="metric-type-badge"]');
      const badgeCount = await typeBadges.count();

      for (let i = 0; i < Math.min(badgeCount, 5); i++) {
        const badge = typeBadges.nth(i);
        const classes = await badge.getAttribute('class');
        // Each badge should have a color styling class
        expect(classes).toMatch(/text-|bg-/);
      }
    }
  });

  test('service filter dropdown populates from API', async () => {
    const serviceSelect = page.locator('select').filter({ hasText: 'All Services' });
    const options = serviceSelect.locator('option');

    // Wait for at least the "All Services" default option to appear
    await expect(options.first()).toHaveText('All Services');
    const optionCount = await options.count();
    expect(optionCount).toBeGreaterThanOrEqual(1);
  });

  test('filtering by service sends correct API request', async () => {
    const serviceSelect = page.locator('select').filter({ hasText: 'All Services' });
    const options = serviceSelect.locator('option');
    const optionCount = await options.count();

    if (optionCount > 1) {
      // Select the first actual service (not "All Services")
      const serviceName = await options.nth(1).textContent();
      await serviceSelect.selectOption(serviceName!);

      const responsePromise = page.waitForResponse((resp) =>
        resp.url().includes('/api/metrics') &&
        resp.url().includes('service=') &&
        resp.status() === 200,
      );
      await page.getByRole('button', { name: 'Search' }).click();
      await responsePromise;
    }
  });

  test('filtering by metric name sends correct API request', async () => {
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await nameInput.fill('http.request');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') &&
      resp.url().includes('name=') &&
      resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;
  });

  test('clear button resets filters', async () => {
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await nameInput.fill('test-metric');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Clear' }).click();
    await responsePromise;

    await expect(nameInput).toHaveValue('');
  });

  test('metric count is displayed in filter bar', async () => {
    const countText = page.locator('[data-testid="metrics-count"]');
    await expect(countText).toBeVisible();
    const text = await countText.textContent();
    expect(text).toMatch(/\d+ metrics?/);
  });

  test('shows empty state when no metrics match filter', async () => {
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await nameInput.fill('nonexistent_metric_xyz_999');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;

    // Should show empty state or zero count
    const countText = page.locator('[data-testid="metrics-count"]');
    const text = await countText.textContent();
    expect(text).toMatch(/0 metrics/);
  });

  test('sidebar highlights the Metrics link', async () => {
    const metricsNav = page.locator('[data-testid="sidebar-nav-item"]').filter({ hasText: 'Metrics' });
    await expect(metricsNav).toHaveAttribute('data-active', 'true');
  });
});
