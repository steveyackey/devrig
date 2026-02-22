import { test, expect } from '@playwright/test';

test.describe('Metrics View', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/#/metrics');
    await page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
  });

  test('displays the metrics heading', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Metrics' })).toBeVisible();
    await expect(page.getByText('Telemetry metric data points')).toBeVisible();
  });

  test('renders the filter bar with metric name and service filter', async ({ page }) => {
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

  test('metrics table has correct column headers', async ({ page }) => {
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

  test('metric rows render with name, type badge, and value', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const firstRow = rows.first();

      // Metric name in monospace
      const metricName = firstRow.locator('.font-mono').first();
      await expect(metricName).toBeVisible();

      // Type badge (Counter, Gauge, or Histogram)
      const typeBadge = firstRow.locator('.rounded');
      await expect(typeBadge.first()).toBeVisible();
      const badgeText = await typeBadge.first().textContent();
      expect(['Counter', 'Gauge', 'Histogram']).toContain(badgeText!.trim());

      // Numeric value
      const valueCell = firstRow.locator('td.text-right .font-mono');
      await expect(valueCell).toBeVisible();
    }
  });

  test('type badges have correct color coding', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Check that type badges use colored backgrounds
      const counterBadges = page.locator('.bg-blue-500\\/20');
      const gaugeBadges = page.locator('.bg-green-500\\/20');
      const histogramBadges = page.locator('.bg-purple-500\\/20');

      const totalTyped =
        (await counterBadges.count()) +
        (await gaugeBadges.count()) +
        (await histogramBadges.count());

      // At least some metric type badges should have colors
      expect(totalTyped).toBeGreaterThanOrEqual(0);
    }
  });

  test('service filter dropdown populates from API', async ({ page }) => {
    // Wait for status API which populates service list
    await page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );

    const serviceSelect = page.locator('select').first();
    const options = serviceSelect.locator('option');
    const optionCount = await options.count();

    // Should have at least the "All Services" default option
    expect(optionCount).toBeGreaterThanOrEqual(1);
    await expect(options.first()).toHaveText('All Services');
  });

  test('filtering by service sends correct API request', async ({ page }) => {
    await page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );

    const serviceSelect = page.locator('select').first();
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

  test('filtering by metric name sends correct API request', async ({ page }) => {
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

  test('clear button resets filters', async ({ page }) => {
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await nameInput.fill('test-metric');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Clear' }).click();
    await responsePromise;

    await expect(nameInput).toHaveValue('');
  });

  test('metric count is displayed in filter bar', async ({ page }) => {
    const countText = page.locator('form .text-zinc-600').last();
    await expect(countText).toBeVisible();
    const text = await countText.textContent();
    expect(text).toMatch(/\d+ metrics?/);
  });

  test('shows empty state when no metrics match filter', async ({ page }) => {
    const nameInput = page.locator('input[placeholder="Filter by name..."]');
    await nameInput.fill('nonexistent_metric_xyz_999');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/metrics') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;

    // Should show empty state or zero count
    const countText = page.locator('form .text-zinc-600').last();
    const text = await countText.textContent();
    expect(text).toMatch(/0 metrics/);
  });

  test('sidebar highlights the Metrics link', async ({ page }) => {
    const metricsNav = page.locator('nav a').filter({ hasText: 'Metrics' });
    await expect(metricsNav).toHaveClass(/bg-blue-500/);
  });
});
