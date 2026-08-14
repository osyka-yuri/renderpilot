import { assertI18nBundleBoundaries, type OutputBundleLike } from './i18n-bundle-boundaries.ts';

export function i18nBundleBoundaryPlugin() {
  return {
    name: 'renderpilot-i18n-bundle-boundaries',
    apply: 'build',
    enforce: 'post',
    generateBundle(_options: unknown, bundle: OutputBundleLike): void {
      assertI18nBundleBoundaries(bundle);
    },
  };
}
