import { test, expect } from '@playwright/test';

test.describe('Overview / Status View', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/#/status');
    // Wait for the status API to respond and the view to render
    await page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );
  });

  test('displays the system status heading', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'System Status' })).toBeVisible();
    await expect(page.getByText('Telemetry pipeline overview')).toBeVisible();
  });

  test('renders stat cards for traces, spans, logs, and metrics', async ({ page }) => {
    // Each stat card has a label rendered as uppercase text
    await expect(page.getByText('Traces', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Spans', { exact: true })).toBeVisible();
    await expect(page.getByText('Logs', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('Metrics', { exact: true }).first()).toBeVisible();
  });

  test('stat cards display numeric values', async ({ page }) => {
    // The stat card values are rendered as large numbers in font-mono
    const statValues = page.locator('.text-2xl.font-semibold.font-mono');
    await expect(statValues).toHaveCount(4);

    for (let i = 0; i < 4; i++) {
      const text = await statValues.nth(i).textContent();
      // Values should be numeric strings (possibly with locale formatting like commas)
      expect(text).toBeTruthy();
      expect(text!.trim()).toMatch(/^[\d,]+$/);
    }
  });

  test('shows reporting services section', async ({ page }) => {
    await expect(page.getByText('Reporting Services')).toBeVisible();
    await expect(
      page.getByText('Services that have sent telemetry data'),
    ).toBeVisible();
  });

  test('services have green status indicator dots', async ({ page }) => {
    // Each service row has a green dot (w-2 h-2 rounded-full bg-green-500)
    const greenDots = page.locator('.bg-green-500.rounded-full');
    const count = await greenDots.count();

    if (count > 0) {
      // At least the first service should have a visible green dot
      await expect(greenDots.first()).toBeVisible();
    }
  });

  test('service rows have View Traces and View Logs links', async ({ page }) => {
    // Wait for potential services to load
    const servicesSection = page.locator('.divide-y');

    const serviceRows = servicesSection.locator('> div');
    const rowCount = await serviceRows.count();

    if (rowCount > 0) {
      const firstRow = serviceRows.first();
      await expect(firstRow.getByText('View Traces')).toBeVisible();
      await expect(firstRow.getByText('View Logs')).toBeVisible();
    }
  });

  test('refresh button triggers data reload', async ({ page }) => {
    const refreshButton = page.getByRole('button', { name: 'Refresh' });
    await expect(refreshButton).toBeVisible();

    // Click refresh and wait for the API call
    const responsePromise = page.waitForResponse((resp) =>
      resp.url().includes('/api/status') && resp.status() === 200,
    );
    await refreshButton.click();
    await responsePromise;
  });

  test('shows auto-refresh indicator', async ({ page }) => {
    await expect(page.getByText('Auto-refreshes every 5 seconds')).toBeVisible();
  });

  test('sidebar navigation highlights the Status link', async ({ page }) => {
    // The Status nav item should have the active styling (blue colors)
    const statusNav = page.locator('nav a').filter({ hasText: 'Status' });
    await expect(statusNav).toHaveClass(/bg-blue-500/);
  });

  test('status bar at the bottom shows telemetry counts', async ({ page }) => {
    const footer = page.locator('footer');
    await expect(footer).toBeVisible();

    // Status bar shows counts for Traces, Spans, Logs, Metrics, Services
    await expect(footer.getByText('Traces:')).toBeVisible();
    await expect(footer.getByText('Spans:')).toBeVisible();
    await expect(footer.getByText('Logs:')).toBeVisible();
    await expect(footer.getByText('Metrics:')).toBeVisible();
    await expect(footer.getByText('Services:')).toBeVisible();
  });
});
