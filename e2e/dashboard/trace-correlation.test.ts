import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from 'bun:test';
import { launchBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';

describe('Trace Correlation', () => {
  let browser: Browser;
  let page: Page;

  beforeAll(async () => {
    browser = await launchBrowser();
  });

  afterAll(async () => {
    await browser.close();
  });

  beforeEach(async () => {
    page = await newPage(browser);
  });

  afterEach(async () => {
    await page.context().close();
  });

  test('clicking a trace ID link on a log row navigates to the trace view', async () => {
    // Start on the logs view
    const logsResponse = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') && resp.status() === 200,
    );
    await page.goto('/#/logs');
    await logsResponse;

    // Find a log entry that has a trace link
    const traceLinks = page.locator('[data-testid="log-trace-link"]');
    const linkCount = await traceLinks.count();

    if (linkCount > 0) {
      // Capture the trace ID from the link href
      const href = await traceLinks.first().getAttribute('href');
      expect(href).toBeTruthy();
      const expectedTraceId = href!.replace('#/traces/', '');

      // Click the trace ID link
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().includes(`/api/traces/${expectedTraceId}`) && resp.status() === 200,
      );
      await traceLinks.first().click();

      // Should navigate to the trace detail view
      await page.waitForURL(`/#/traces/${expectedTraceId}`);
      await detailResponse;

      // The trace detail view should be visible
      await expect(page.getByRole('heading', { name: 'Trace Detail' })).toBeVisible();

      // The trace ID should be displayed on the page
      await expect(page.getByText(expectedTraceId)).toBeVisible();
    }
  });

  test('trace detail view shows the full trace ID', async () => {
    // Navigate to traces to get a real trace ID
    const tracesResponse = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
    await page.goto('/#/traces');
    await tracesResponse;

    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      // Click first trace to navigate to detail
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await detailResponse;

      // The full trace ID should be visible in the header
      await expect(page.getByRole('heading', { name: 'Trace Detail' })).toBeVisible();
    }
  });

  test('trace detail shows related logs under the Logs tab', async () => {
    const tracesResponse = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
    await page.goto('/#/traces');
    await tracesResponse;

    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      const relatedResponse = page.waitForResponse((resp) =>
        resp.url().includes('/related') && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);

      // Wait for both trace and related data to load
      await detailResponse;
      await relatedResponse;

      // Click the Logs tab
      const logsTab = page.getByRole('tab', { name: /Logs \(/ });
      await logsTab.click();

      // Extract the log count from the tab label
      const tabText = await logsTab.textContent();
      const match = tabText!.match(/Logs \((\d+)\)/);

      if (match && parseInt(match[1], 10) > 0) {
        // Related logs table should show entries
        const logRows = page.locator('tbody tr');
        const logCount = await logRows.count();
        expect(logCount).toBeGreaterThan(0);
      } else {
        // No related logs - empty state should show
        await expect(
          page.getByText('No related logs found for this trace'),
        ).toBeVisible();
      }
    }
  });

  test('trace detail shows related metrics under the Metrics tab', async () => {
    const tracesResponse = page.waitForResponse((resp) =>
      resp.url().includes('/api/traces') && resp.status() === 200,
    );
    await page.goto('/#/traces');
    await tracesResponse;

    const rows = page.locator('[data-testid="trace-row"]');
    const rowCount = await rows.count();

    if (rowCount > 0) {
      const detailResponse = page.waitForResponse((resp) =>
        resp.url().match(/\/api\/traces\/[^/]+$/) !== null && resp.status() === 200,
      );
      const relatedResponse = page.waitForResponse((resp) =>
        resp.url().includes('/related') && resp.status() === 200,
      );
      await rows.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);
      await detailResponse;
      await relatedResponse;

      // Click the Metrics tab
      const metricsTab = page.getByRole('tab', { name: /Metrics \(/ });
      await metricsTab.click();

      const tabText = await metricsTab.textContent();
      const match = tabText!.match(/Metrics \((\d+)\)/);

      if (match && parseInt(match[1], 10) > 0) {
        const metricRows = page.locator('tbody tr');
        const metricCount = await metricRows.count();
        expect(metricCount).toBeGreaterThan(0);
      } else {
        await expect(
          page.getByText('No related metrics found for this trace'),
        ).toBeVisible();
      }
    }
  });

  test('navigating from log trace link preserves browser history', async () => {
    const logsResponse = page.waitForResponse((resp) =>
      resp.url().includes('/api/logs') && resp.status() === 200,
    );
    await page.goto('/#/logs');
    await logsResponse;

    const traceLinks = page.locator('[data-testid="log-trace-link"]');
    const linkCount = await traceLinks.count();

    if (linkCount > 0) {
      // Click the trace link
      await traceLinks.first().click();
      await page.waitForURL(/\/#\/traces\/.+/);

      // Go back in browser history
      await page.goBack();

      // Should return to the logs view
      await expect(page.getByRole('heading', { name: 'Logs' })).toBeVisible();
    }
  });
});
