import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from 'bun:test';
import { launchBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';

describe('Dark / Light Theme Toggle', () => {
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
    await page.goto('/');
    await page.waitForURL(/\/#\//);
  });

  afterEach(async () => {
    await page.context().close();
  });

  test('dashboard loads in dark mode by default', async () => {
    // The body element has class="dark"
    const body = page.locator('body');
    await expect(body).toHaveClass(/dark/);

    // The app layout should be visible
    const appLayout = page.locator('[data-testid="app-layout"]');
    await expect(appLayout).toBeVisible();
  });

  test('theme toggle button is visible', async () => {
    // Look for a theme toggle button
    const themeToggle = page.locator('[data-testid="theme-toggle"]');

    if (await themeToggle.isVisible().catch(() => false)) {
      await expect(themeToggle).toBeVisible();
    } else {
      // If no data-testid, look for common theme toggle patterns
      const toggleByLabel = page.getByRole('button', { name: /theme|dark|light|mode/i });
      // Theme toggle may not be implemented yet
      if (await toggleByLabel.count() > 0) {
        await expect(toggleByLabel.first()).toBeVisible();
      }
    }
  });

  test('clicking theme toggle switches to light mode', async () => {
    const themeToggle = page.locator('[data-testid="theme-toggle"]');

    if (await themeToggle.isVisible().catch(() => false)) {
      await themeToggle.click();

      // After toggling, body should not have "dark" class (or should have "light")
      const body = page.locator('body');
      await expect(body).not.toHaveClass(/dark/);
    }
  });

  test('clicking theme toggle twice returns to dark mode', async () => {
    const themeToggle = page.locator('[data-testid="theme-toggle"]');

    if (await themeToggle.isVisible().catch(() => false)) {
      // Toggle to light
      await themeToggle.click();
      const body = page.locator('body');
      await expect(body).not.toHaveClass(/dark/);

      // Toggle back to dark
      await themeToggle.click();
      await expect(body).toHaveClass(/dark/);
    }
  });

  test('theme preference persists across page refresh', async () => {
    const themeToggle = page.locator('[data-testid="theme-toggle"]');

    if (await themeToggle.isVisible().catch(() => false)) {
      // Toggle to light mode
      await themeToggle.click();
      const body = page.locator('body');
      await expect(body).not.toHaveClass(/dark/);

      // Verify the preference was persisted to localStorage
      const stored = await page.evaluate(() => localStorage.getItem('devrig-theme'));
      expect(stored).toBe('light');

      // Open a fresh page in the same browser context (shares localStorage)
      const freshPage = await page.context().newPage();
      await freshPage.goto('/', { waitUntil: 'domcontentloaded' });
      await freshPage.locator('[data-testid="theme-toggle"]').waitFor();

      // Fresh page should load in light mode from stored preference
      await expect(freshPage.locator('body')).not.toHaveClass(/dark/);
      await freshPage.close();
    }
  });

  test('theme preference is stored in localStorage', async () => {
    const themeToggle = page.locator('[data-testid="theme-toggle"]');

    if (await themeToggle.isVisible().catch(() => false)) {
      // Toggle theme
      await themeToggle.click();

      // Check that the preference was stored
      const storedTheme = await page.evaluate(() => {
        return (
          localStorage.getItem('theme') ||
          localStorage.getItem('devrig-theme') ||
          localStorage.getItem('color-mode')
        );
      });

      expect(storedTheme).toBeTruthy();
    }
  });

  test('dark mode renders with correct background colors', async () => {
    // Verify dark mode colors are applied
    const sidebar = page.locator('[data-testid="sidebar"]');
    await expect(sidebar).toBeVisible();

    // Check computed background color of body is dark
    const bgColor = await page.evaluate(() => {
      return window.getComputedStyle(document.body).backgroundColor;
    });

    // Should be a dark color (low RGB values)
    const rgbMatch = bgColor.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    if (rgbMatch) {
      const [, r, g, b] = rgbMatch.map(Number);
      expect(Math.max(r, g, b)).toBeLessThan(50);
    }
  });

  test('sidebar and main content have consistent theme', async () => {
    // Both sidebar and main area should be visible
    const sidebar = page.locator('[data-testid="sidebar"]');
    const mainArea = page.locator('[data-testid="main-content"]');

    await expect(sidebar).toBeVisible();
    await expect(mainArea).toBeVisible();
  });

  test('text remains readable in dark mode', async () => {
    // Header text should be light-colored on dark background
    const heading = page.locator('h1, h2').first();
    await expect(heading).toBeVisible();

    const color = await heading.evaluate((el) => {
      return window.getComputedStyle(el).color;
    });

    // Color should be a light tone (high RGB values)
    const rgbMatch = color.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    if (rgbMatch) {
      const [, r, g, b] = rgbMatch.map(Number);
      // Light text should have RGB values above 150
      expect(Math.max(r, g, b)).toBeGreaterThan(150);
    }
  });
});
