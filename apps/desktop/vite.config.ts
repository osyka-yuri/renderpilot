import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

const PROJECT_ROOT = path.dirname(fileURLToPath(import.meta.url));
const UI_SOURCE_ROOT = path.resolve(PROJECT_ROOT, 'ui/src');

const DEV_SERVER_PORT = 1420;
const DEV_SERVER_HMR_PORT = 1421;

const TAURI_SOURCE_GLOB = '**/src-tauri/**';
const TEST_FILE_GLOB = 'ui/src/**/*.test.ts';

const LAYER_ALIAS_PATHS = {
  '@app': 'app',
  '@pages': 'pages',
  '@widgets': 'widgets',
  '@features': 'features',
  '@entities': 'entities',
  '@shared': 'shared',
} as const satisfies Record<string, string>;

function readOptionalEnv(name: string): string | undefined {
  const value = process.env[name]?.trim();

  return value ?? undefined;
}

function createLayerAliases(
  sourceRoot: string,
  layerAliasPaths: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(layerAliasPaths).map(([alias, relativePath]) => [
      alias,
      path.resolve(sourceRoot, relativePath),
    ]),
  );
}

const devHost = readOptionalEnv('TAURI_DEV_HOST');

const hmrConfig = devHost
  ? {
      protocol: 'ws' as const,
      host: devHost,
      port: DEV_SERVER_HMR_PORT,
    }
  : undefined;

export default defineConfig({
  clearScreen: false,

  plugins: [tailwindcss(), svelte()],

  resolve: {
    alias: createLayerAliases(UI_SOURCE_ROOT, LAYER_ALIAS_PATHS),
  },

  server: {
    port: DEV_SERVER_PORT,
    strictPort: true,
    host: devHost ?? false,
    hmr: hmrConfig,
    watch: {
      ignored: [TAURI_SOURCE_GLOB],
    },
  },

  test: {
    environment: 'node',
    include: [TEST_FILE_GLOB],
  },
});
