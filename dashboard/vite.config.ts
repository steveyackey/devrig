import { defineConfig } from 'vite';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solidPlugin()],
  build: {
    outDir: 'dist',
    target: 'esnext',
  },
  server: {
    proxy: {
      '/api': 'http://localhost:9500',
      '/ws': { target: 'ws://localhost:9500', ws: true },
    },
  },
});
