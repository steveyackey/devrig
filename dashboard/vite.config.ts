import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  plugins: [tailwindcss(), solidPlugin()],
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
