import { test, expect } from '@playwright/test';

test.describe('Traces View', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/#/traces');
    await page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
  });

  test('displays the traces heading', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Traces' })).toBeVisible();
    await expect(page.getByText('Distributed trace overview')).toBeVisible();
  });

  test('renders the filter bar with service, status, and duration filters', async ({ page }) => {
    // Service dropdown
    const serviceSelect = page.locator('select').filter({ hasText: 'All Services' });
    await expect(serviceSelect).toBeVisible();

    // Status dropdown
    const statusSelect = page.locator('select').filter({ hasText: 'All' });
    await expect(statusSelect.first()).toBeVisible();

    // Min Duration input
    const durationInput = page.locator('input[type="number"][placeholder="ms"]');
    await expect(durationInput).toBeVisible();

    // Search button
    await expect(page.getByRole('button', { name: 'Search' })).toBeVisible();

    // Clear button
    await expect(page.getByRole('button', { name: 'Clear' })).toBeVisible();
  });

  test('trace table has correct column headers', async ({ page }) => {
    const headers = page.locator('thead th');
    const headerTexts = await headers.allTextContents();
    const normalized = headerTexts.map((t) => t.trim().toLowerCase());

    expect(normalized).toContain('trace id');
    expect(normalized).toContain('operation');
    expect(normalized).toContain('services');
    expect(normalized).toContain('duration');
    expect(normalized).toContain('spans');
    expect(normalized).toContain('status');
    expect(normalized).toContain('time');
  });

  test('trace rows render with trace ID, service tags, and status badges', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const firstRow = rows.first();

      // Trace ID is a blue monospace link
      const traceId = firstRow.locator('.font-mono.text-blue-400');
      await expect(traceId).toBeVisible();

      // Status badge should show either Ok or Error
      const statusBadge = firstRow.locator('.rounded-full');
      await expect(statusBadge).toBeVisible();
      const badgeText = await statusBadge.textContent();
      expect(['Ok', 'Error']).toContain(badgeText!.trim());
    }
  });

  test('waterfall renders on trace detail navigation', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Click the first trace row to navigate to detail
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);

      // Wait for the trace detail API to respond
      await page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );

      // The trace detail page should show the waterfall
      await expect(page.getByRole('heading', { name: 'Trace Detail' })).toBeVisible();

      // Spans tab should be active and show span count
      const spansTab = page.getByRole('button', { name: /Spans \(/ });
      await expect(spansTab).toBeVisible();

      // Waterfall bars should be present (the timeline bar containers)
      const waterfallBars = page.locator('.relative.h-6');
      const barCount = await waterfallBars.count();
      expect(barCount).toBeGreaterThan(0);
    }
  });

  test('filters narrow trace results', async ({ page }) => {
    // Filter by status = Error
    const statusSelect = page.locator('select').nth(1);
    await statusSelect.selectOption('Error');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') &&
      resp.url().includes('status=Error') &&
      resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;

    // All visible status badges should be Error (or no results)
    const badges = page.locator('tbody .rounded-full');
    const count = await badges.count();
    for (let i = 0; i < count; i++) {
      await expect(badges.nth(i)).toHaveText('Error');
    }
  });

  test('clear button resets all filters', async ({ page }) => {
    // Set a filter
    const statusSelect = page.locator('select').nth(1);
    await statusSelect.selectOption('Ok');

    // Clear filters
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Clear' }).click();
    await responsePromise;

    // Status select should be back to empty (All)
    await expect(statusSelect).toHaveValue('');
  });

  test('span detail panel opens when clicking a span in waterfall', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Navigate to trace detail
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );

      // Click on a span row in the waterfall
      const spanRows = page.locator('.cursor-pointer').filter({ has: page.locator('.relative.h-6') });
      const spanCount = await spanRows.count();

      if (spanCount > 0) {
        await spanRows.first().click();

        // Span detail panel should appear
        await expect(page.getByRole('heading', { name: 'Span Details' })).toBeVisible();

        // Detail panel should show span attributes
        await expect(page.getByText('Service')).toBeVisible();
        await expect(page.getByText('Operation')).toBeVisible();
        await expect(page.getByText('Span ID')).toBeVisible();
        await expect(page.getByText('Duration')).toBeVisible();
      }
    }
  });

  test('trace detail shows tabs for Spans, Logs, and Metrics', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );

      // All three tabs should be present
      await expect(page.getByRole('button', { name: /Spans \(/ })).toBeVisible();
      await expect(page.getByRole('button', { name: /Logs \(/ })).toBeVisible();
      await expect(page.getByRole('button', { name: /Metrics \(/ })).toBeVisible();
    }
  });

  test('back to traces link navigates away from detail', async ({ page }) => {
    const rows = page.locator('tbody tr');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);

      // Click "Back to Traces"
      await page.getByText('Back to Traces').click();
      await page.waitForURL('/#/traces');

      await expect(page.getByRole('heading', { name: 'Traces' })).toBeVisible();
    }
  });

  test('trace count is displayed in filter bar', async ({ page }) => {
    // Should show something like "5 traces" or "0 traces"
    const countText = page.locator('form .text-zinc-600').last();
    await expect(countText).toBeVisible();
    const text = await countText.textContent();
    expect(text).toMatch(/\d+ traces?/);
  });
});
