/**
 * Desktop UI formatting.
 *
 * Keep in sync with package.json scripts:
 * - format
 * - format:check
 *
 * Plugin order is intentional:
 * - prettier-plugin-svelte formats Svelte files.
 * - prettier-plugin-tailwindcss sorts Tailwind classes and should stay last.
 *
 * @see https://prettier.io/docs/configuration
 * @type {import('prettier').Config & import('prettier-plugin-tailwindcss').PluginOptions}
 */
const config = {
  printWidth: 100,
  tabWidth: 2,
  useTabs: false,

  semi: true,
  singleQuote: true,
  trailingComma: 'all',
  endOfLine: 'lf',

  plugins: ['prettier-plugin-svelte', 'prettier-plugin-tailwindcss'],

  // Tailwind CSS v4 entry point.
  // Paths are resolved relative to this Prettier config file.
  tailwindStylesheet: './ui/src/shared/theme/global.css',

  // Sort Tailwind classes inside helper calls.
  tailwindFunctions: ['cva', 'cn', 'clsx', 'cx'],
};

export default config;
