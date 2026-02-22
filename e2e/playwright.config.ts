import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './dashboard',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  grepInvert: process.env.SCREENSHOTS ? undefined : /@screenshots/,
  retries: 1,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  timeout: 30_000,

  use: {
    baseURL: 'http://localhost:4000',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    colorScheme: 'dark',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  webServer: {
    command: 'cargo run -- start',
    cwd: '..',
    url: 'http://localhost:4000/api/status',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
