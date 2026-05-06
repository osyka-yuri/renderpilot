import path from 'node:path';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  plugins: [svelte()],
  resolve: {
    alias: {
      '@app': path.resolve(__dirname, 'ui/src/app'),
      '@features': path.resolve(__dirname, 'ui/src/features'),
      '@shared': path.resolve(__dirname, 'ui/src/shared'),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
});