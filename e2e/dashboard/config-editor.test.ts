import { describe, test, expect, beforeAll, afterAll, beforeEach, afterEach } from 'bun:test';
import { launchBrowser, newPage } from '../helpers';
import type { Browser, Page } from 'playwright';

describe('Config Editor', () => {
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
    const responsePromise = page.waitForResponse(
      (resp) => resp.url().includes('/api/config') && resp.status() === 200,
    );
    await page.goto('/#/config');
    await responsePromise;
  });

  afterEach(async () => {
    await page.context().close();
  });

  test('config editor loads current config', async () => {
    // The config view heading should be visible
    await expect(page.getByRole('heading', { name: 'Configuration' })).toBeVisible();
    await expect(page.getByText('Edit devrig.toml')).toBeVisible();

    // The editor should be present with CodeMirror content
    const editor = page.locator('.cm-editor');
    await expect(editor).toBeVisible();

    // The save button should be present
    await expect(page.getByRole('button', { name: 'Save' })).toBeVisible();
  });

  test('shows validation error for invalid TOML', async () => {
    // Wait for the editor to be ready
    const editor = page.locator('.cm-editor');
    await expect(editor).toBeVisible();

    // Focus the editor and type invalid TOML
    const cmContent = page.locator('.cm-content');
    await cmContent.click();

    // Select all and replace with invalid content
    await page.keyboard.press('Meta+a');
    await page.keyboard.press('Control+a');
    await page.keyboard.type('this is not [valid toml =');

    // Should show a TOML error message
    await expect(page.getByText(/TOML error/i)).toBeVisible({ timeout: 5000 });

    // Save button should be disabled when there's a validation error
    const saveButton = page.getByRole('button', { name: 'Save' });
    await expect(saveButton).toBeDisabled();
  });

  test('save button persists changes', async () => {
    const editor = page.locator('.cm-editor');
    await expect(editor).toBeVisible();

    // Focus the editor
    const cmContent = page.locator('.cm-content');
    await cmContent.click();

    // Move to end and add a comment
    await page.keyboard.press('Meta+End');
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('# test comment');

    // Click the save button
    const saveButton = page.getByRole('button', { name: 'Save' });
    await expect(saveButton).toBeEnabled();

    const responsePromise = page.waitForResponse(
      (resp) => resp.url().includes('/api/config') && resp.request().method() === 'PUT',
    );
    await saveButton.click();
    const response = await responsePromise;

    // Save should succeed (200)
    expect(response.status()).toBe(200);

    // Should show "Saved" status or "Configuration saved" toast
    await expect(
      page.getByText('Saved').first(),
    ).toBeVisible({ timeout: 5000 });
  });
});
