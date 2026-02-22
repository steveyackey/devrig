import { test, expect } from '@playwright/test';

test.describe('Logs View', () => {
  test.beforeEach(async ({ page }) => {
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') && resp.status() === 200,
    );
    await page.goto('/#/logs');
    await responsePromise;
  });

  test('displays the logs heading', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Logs' })).toBeVisible();
    await expect(page.getByText('Application log records')).toBeVisible();
  });

  test('renders the filter bar with service, severity, and search', async ({ page }) => {
    // Service dropdown
    const serviceSelect = page.locator('select').filter({ hasText: 'All Services' });
    await expect(serviceSelect).toBeVisible();

    // Severity dropdown
    const severitySelect = page.locator('select').nth(1);
    await expect(severitySelect).toBeVisible();

    // Search input
    const searchInput = page.locator('input[placeholder="Search log body..."]');
    await expect(searchInput).toBeVisible();

    // Buttons
    await expect(page.getByRole('button', { name: 'Search' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Clear' })).toBeVisible();
  });

  test('log table has correct column headers', async ({ page }) => {
    const headers = page.locator('thead th');
    const headerTexts = await headers.allTextContents();
    const normalized = headerTexts.map((t) => t.trim().toLowerCase());

    expect(normalized).toContain('time');
    expect(normalized).toContain('severity');
    expect(normalized).toContain('service');
    expect(normalized).toContain('body');
    expect(normalized).toContain('trace');
  });

  test('log lines appear with timestamp, severity badge, and body', async ({ page }) => {
    const rows = page.locator('[data-testid="log-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const firstRow = rows.first();

      // Timestamp
      const timestamp = firstRow.locator('[data-testid="log-timestamp"]');
      await expect(timestamp).toBeVisible();

      // Severity badge
      const severityBadge = firstRow.locator('[data-testid="log-severity-badge"]');
      await expect(severityBadge).toBeVisible();
      const badgeText = await severityBadge.textContent();
      expect(['Trace', 'Debug', 'Info', 'Warn', 'Error', 'Fatal']).toContain(
        badgeText!.trim(),
      );

      // Body text
      const body = firstRow.locator('[data-testid="log-body"]');
      await expect(body).toBeVisible();
    }
  });

  test('severity badges have correct color coding', async ({ page }) => {
    const rows = page.locator('[data-testid="log-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Check that severity badges exist
      const badges = page.locator('[data-testid="log-severity-badge"]');
      const badgeCount = await badges.count();
      expect(badgeCount).toBeGreaterThan(0);

      for (let i = 0; i < Math.min(badgeCount, 5); i++) {
        const badge = badges.nth(i);
        const classes = await badge.getAttribute('class');

        // Each badge should have a background color class
        expect(classes).toMatch(/bg-/);
      }
    }
  });

  test('severity filter sends correct API request', async ({ page }) => {
    const severitySelect = page.locator('select').nth(1);
    await severitySelect.selectOption('Error');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') &&
      resp.url().includes('severity=Error') &&
      resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;

    // All visible severity badges should be Error (or no results)
    const badges = page.locator('[data-testid="log-severity-badge"]');
    const count = await badges.count();
    for (let i = 0; i < count; i++) {
      await expect(badges.nth(i)).toHaveText('Error');
    }
  });

  test('severity dropdown contains all severity levels', async ({ page }) => {
    const severitySelect = page.locator('select').nth(1);
    const options = severitySelect.locator('option');
    const optionTexts = await options.allTextContents();

    expect(optionTexts).toContain('All');
    expect(optionTexts).toContain('Trace');
    expect(optionTexts).toContain('Debug');
    expect(optionTexts).toContain('Info');
    expect(optionTexts).toContain('Warn');
    expect(optionTexts).toContain('Error');
    expect(optionTexts).toContain('Fatal');
  });

  test('search filter sends query to API', async ({ page }) => {
    const searchInput = page.locator('input[placeholder="Search log body..."]');
    await searchInput.fill('connection');

    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') &&
      resp.url().includes('search=') &&
      resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Search' }).click();
    await responsePromise;
  });

  test('clear button resets all filters', async ({ page }) => {
    // Set filters
    const severitySelect = page.locator('select').nth(1);
    await severitySelect.selectOption('Warn');

    const searchInput = page.locator('input[placeholder="Search log body..."]');
    await searchInput.fill('some search');

    // Clear
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') && resp.status() === 200,
    );
    await page.getByRole('button', { name: 'Clear' }).click();
    await responsePromise;

    await expect(severitySelect).toHaveValue('');
    await expect(searchInput).toHaveValue('');
  });

  test('log count is displayed in filter bar', async ({ page }) => {
    const countText = page.locator('[data-testid="logs-count"]');
    await expect(countText).toBeVisible();
    const text = await countText.textContent();
    expect(text).toMatch(/\d+ logs?/);
  });

  test('logs with trace IDs show clickable trace links', async ({ page }) => {
    const traceLinks = page.locator('[data-testid="log-trace-link"]');
    const linkCount = await traceLinks.count();

    if (linkCount > 0) {
      const firstLink = traceLinks.first();
      await expect(firstLink).toBeVisible();

      // The link text should be a truncated trace ID (8 chars + ...)
      const linkText = await firstLink.textContent();
      expect(linkText).toMatch(/^[a-f0-9]+\.\.\.$/);
    }
  });

  test('logs without trace IDs show a dash', async ({ page }) => {
    // Some logs may not have trace IDs and should show "-"
    const logRows = page.locator('[data-testid="log-row"]');
    const rowCount = await logRows.count();

    for (let i = 0; i < rowCount; i++) {
      const row = logRows.nth(i);
      const traceLink = row.locator('[data-testid="log-trace-link"]');
      if ((await traceLink.count()) === 0) {
        // Row without trace link should have a dash
        const lastTd = row.locator('td:last-child span');
        if ((await lastTd.count()) > 0) {
          await expect(lastTd.first()).toHaveText('-');
        }
        break;
      }
    }
  });

  test('sidebar highlights the Logs link', async ({ page }) => {
    const logsNav = page.locator('[data-testid="sidebar-nav-item"]').filter({ hasText: 'Logs' });
    await expect(logsNav).toHaveAttribute('data-active', 'true');
  });
});
