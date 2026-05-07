/**
 * Desktop UI formatting. Keep in sync with scripts in package.json (`format`, `format:check`).
 * @see https://prettier.io/docs/en/options.html
 * @type {import('prettier').Config}
 */
export default {
  semi: true,
  singleQuote: true,
  tabWidth: 2,
  trailingComma: 'all',
  printWidth: 100,
  endOfLine: 'lf',
  plugins: ['prettier-plugin-svelte'],
};
