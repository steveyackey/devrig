import { chromium, type Browser, type Page } from 'playwright';

const ci = !!process.env.CI;
const screenshots = !!process.env.SCREENSHOTS;
const useVite = !ci && screenshots;

export const BASE_URL = useVite ? 'http://localhost:5173' : 'http://localhost:4000';

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
