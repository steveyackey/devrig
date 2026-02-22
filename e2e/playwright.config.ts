import { defineConfig, devices } from '@playwright/test';

const ci = !!process.env.CI;
const screenshots = !!process.env.SCREENSHOTS;

// Use Vite (:5173) only for screenshots in dev — captures live source changes.
// All other tests use the embedded server (:4000) which is 10x faster.
const useVite = !ci && screenshots;

export default defineConfig({
  testDir: './dashboard',
  fullyParallel: true,
  forbidOnly: ci,
  grepInvert: screenshots ? undefined : /@screenshots/,
  retries: 1,
  workers: ci ? 1 : undefined,
  reporter: 'html',
  timeout: 30_000,

  use: {
    baseURL: useVite ? 'http://localhost:5173' : 'http://localhost:4000',
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
    command: useVite
      ? 'cargo run -- start --dev -f devrig.run.toml'
      : 'cargo run -- start -f devrig.run.toml',
    cwd: '..',
    url: 'http://localhost:4000/api/status',
    reuseExistingServer: !ci,
    timeout: 120_000,
  },
});
