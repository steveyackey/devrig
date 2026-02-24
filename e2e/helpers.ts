import { chromium, type Browser, type Page } from 'playwright';

function detectBaseUrl(): string {
  // Check if Vite dev server is running on :5173
  try {
    const res = Bun.spawnSync(['curl', '-sf', '--max-time', '1', 'http://localhost:5173']);
    if (res.exitCode === 0) return 'http://localhost:5173';
  } catch {}
  return 'http://localhost:4000';
}

export const BASE_URL = detectBaseUrl();

let _browser: Browser | null = null;

/** Returns a shared browser instance (launched once, reused across all test files). */
export async function sharedBrowser(): Promise<Browser> {
  if (!_browser) {
    _browser = await chromium.launch({
      headless: !process.env.HEADED,
    });
  }
  return _browser;
}

/** Launch a dedicated browser (use only when you need isolated browser state). */
export async function launchBrowser(): Promise<Browser> {
  return chromium.launch({
    headless: !process.env.HEADED,
  });
}

export async function newPage(browser: Browser): Promise<Page> {
  const context = await browser.newContext({
    colorScheme: 'dark',
    baseURL: BASE_URL,
    viewport: { width: 1280, height: 720 },
  });
  return context.newPage();
}
