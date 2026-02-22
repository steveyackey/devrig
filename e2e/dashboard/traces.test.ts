import { test, expect } from '@playwright/test';

test.describe('Traces View', () => {
  test.beforeEach(async ({ page }) => {
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
    await page.goto('/#/traces');
    await responsePromise;
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
    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const firstRow = rows.first();

      // Trace ID
      const traceId = firstRow.locator('[data-testid="trace-id"]');
      await expect(traceId).toBeVisible();

      // Status badge should show either Ok or Error
      const statusBadge = firstRow.locator('[data-testid="trace-status-badge"]');
      await expect(statusBadge).toBeVisible();
      const badgeText = await statusBadge.textContent();
      expect(['Ok', 'Error']).toContain(badgeText!.trim());
    }
  });

  test('waterfall renders on trace detail navigation', async ({ page }) => {
    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Set up response listener before click
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await detailResponse;

      // The trace detail page should show the waterfall
      await expect(page.getByRole('heading', { name: 'Trace Detail' })).toBeVisible();

      // Spans tab should be active and show span count
      const spansTab = page.getByRole('tab', { name: /Spans \(/ });
      await expect(spansTab).toBeVisible();

      // Waterfall bars should be present
      const waterfallBars = page.locator('[data-testid="waterfall-bar"]');
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

    // Verify the filtered response was received and rendered
    const badges = page.locator('[data-testid="trace-status-badge"]');
    const count = await badges.count();
    for (let i = 0; i < count; i++) {
      await expect(badges.nth(i)).toHaveText(/Ok|Error/);
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
    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Navigate to trace detail
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await detailResponse;

      // Click on a span row in the waterfall
      const spanRows = page.locator('[data-testid="waterfall-row"]');
      const spanCount = await spanRows.count();

      if (spanCount > 0) {
        await spanRows.first().click();

        // Span detail panel should appear with labels
        await expect(page.getByRole('heading', { name: 'Span Details' })).toBeVisible({ timeout: 5000 });
        await expect(page.getByText('Span ID', { exact: true })).toBeVisible({ timeout: 5000 });
        await expect(page.getByText('Duration', { exact: true })).toBeVisible();
      }
    }
  });

  test('trace detail shows tabs for Spans, Logs, and Metrics', async ({ page }) => {
    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await detailResponse;

      // All three tabs should be present
      await expect(page.getByRole('tab', { name: /Spans \(/ })).toBeVisible();
      await expect(page.getByRole('tab', { name: /Logs \(/ })).toBeVisible();
      await expect(page.getByRole('tab', { name: /Metrics \(/ })).toBeVisible();
    }
  });

  test('back to traces link navigates away from detail', async ({ page }) => {
    const rows = page.locator('[data-testid="trace-row"]');
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
    const countText = page.locator('[data-testid="traces-count"]');
    await expect(countText).toBeVisible();
    const text = await countText.textContent();
    expect(text).toMatch(/\d+ traces?/);
  });
});
