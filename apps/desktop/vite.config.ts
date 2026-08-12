import path from 'node:path';
import process from 'node:process';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

import tauriConfig from './src-tauri/tauri.conf.json' with { type: 'json' };
import { i18nBundleBoundaryPlugin } from './scripts/i18n-bundle-boundary-plugin';
import { edgeBuildTarget } from './scripts/webview-runtime-contract';

const PROJECT_ROOT = import.meta.dirname;
const UI_SOURCE_ROOT = path.resolve(PROJECT_ROOT, 'ui/src');

const DEV_SERVER_PORT = 1420;
const DEV_SERVER_HMR_PORT = 1421;

const TAURI_SOURCE_GLOB = '**/src-tauri/**';
const TEST_FILE_GLOBS = ['ui/src/**/*.test.ts', 'scripts/**/*.test.ts', 'eslint/**/*.test.js'];
const TEST_SETUP_FILE = './ui/test-setup.ts';

const WEBVIEW_BUILD_TARGET = edgeBuildTarget(tauriConfig.bundle.windows.minimumWebview2Version);

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

  return value || undefined;
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

  plugins: [tailwindcss(), svelte(), i18nBundleBoundaryPlugin()],

  resolve: {
    alias: createLayerAliases(UI_SOURCE_ROOT, LAYER_ALIAS_PATHS),
    conditions: ['browser'],
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

  build: {
    manifest: true,
    target: WEBVIEW_BUILD_TARGET,
  },

  test: {
    environment: 'node',
    include: TEST_FILE_GLOBS,
    setupFiles: [TEST_SETUP_FILE],
  },
});
