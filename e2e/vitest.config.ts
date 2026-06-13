import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['dashboard/**/*.test.ts'],
    exclude: ['**/node_modules/**'],
    setupFiles: ['./setup.ts'],
    testTimeout: 30000,
    hookTimeout: 30000,
    // Tests share one devrig instance (and the config-editor test writes the
    // config file), so run files sequentially like the previous runner did.
    fileParallelism: false,
  },
});
