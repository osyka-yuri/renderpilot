import assert from 'node:assert/strict';
import test from 'node:test';

import { LAZY_LOCALES } from '../ui/src/shared/i18n/locale-model.ts';
import { assertI18nBundleBoundaries } from './i18n-bundle-boundaries.ts';

const LOCALES = LAZY_LOCALES;

function chunk(fileName, overrides = {}) {
  return {
    type: 'chunk',
    fileName,
    facadeModuleId: null,
    imports: [],
    dynamicImports: [],
    isEntry: false,
    isDynamicEntry: false,
    modules: {},
    ...overrides,
  };
}

function createBundle() {
  const packFiles = LOCALES.map((locale) => `${locale}.js`);
  const bundle = {
    'bootstrap.js': chunk('bootstrap.js', {
      facadeModuleId: '/ui/src/main.ts',
      isEntry: true,
    }),
    'desktop.js': chunk('desktop.js', {
      facadeModuleId: '/ui/src/app/routes/DesktopApp.svelte',
      imports: ['registry.js', 'initial-shared.js'],
      modules: {
        '/ui/src/shared/i18n/packs/en.ts': {},
        '/ui/src/shared/i18n/messages/en.ts': {},
      },
    }),
    'registry.js': chunk('registry.js', {
      dynamicImports: packFiles,
      modules: { '/ui/src/shared/i18n/packs/registry.ts': {} },
    }),
    'initial-shared.js': chunk('initial-shared.js', {
      modules: { '/ui/src/shared/i18n/messages/generated/contract-version.ts': {} },
    }),
    'index.html': {
      type: 'asset',
      fileName: 'index.html',
      source: '<link rel="modulepreload" href="initial-shared.js">',
    },
  };

  for (const locale of LOCALES) {
    bundle[`${locale}.js`] = chunk(`${locale}.js`, {
      facadeModuleId: `/ui/src/shared/i18n/packs/${locale}.ts`,
      imports: ['initial-shared.js'],
      isDynamicEntry: true,
      modules: { [`/ui/src/shared/i18n/messages/${locale}.ts`]: {} },
    });
  }
  return bundle;
}

test('allows locale packs to share an initial locale-neutral chunk', () => {
  assert.doesNotThrow(() => assertI18nBundleBoundaries(createBundle()));
});

test('ignores chunk-name text outside modulepreload href attributes', () => {
  const bundle = createBundle();
  bundle['index.html'].source += '<meta name="diagnostic" content="ru.js">';

  assert.doesNotThrow(() => assertI18nBundleBoundaries(bundle));
});

test('requires an inspectable index.html asset', () => {
  const bundle = createBundle();
  delete bundle['index.html'];

  assert.throws(
    () => assertI18nBundleBoundaries(bundle),
    /Expected index\.html in the generated bundle/,
  );
});

test('rejects locale-specific modules in the initial graph', () => {
  const bundle = createBundle();
  bundle['initial-shared.js'].modules['/ui/src/shared/i18n/messages/ru.ts'] = {};

  assert.throws(
    () => assertI18nBundleBoundaries(bundle),
    /Non-active locale modules leaked into the initial graph/,
  );
});

test('requires every locale pack to remain a direct dynamic import', () => {
  const bundle = createBundle();
  bundle['registry.js'].dynamicImports = bundle['registry.js'].dynamicImports.filter(
    (file) => file !== 'ru.js',
  );
  bundle['registry.js'].dynamicImports.push('bridge.js');
  bundle['bridge.js'] = chunk('bridge.js', { dynamicImports: ['ru.js'] });

  assert.throws(
    () => assertI18nBundleBoundaries(bundle),
    /Locale pack ru is not a direct dynamic import of the locale loader registry/,
  );
});

test('rejects a non-initial locale dependency preloaded by index.html', () => {
  const bundle = createBundle();
  bundle['lazy-extra.js'] = chunk('lazy-extra.js');
  bundle['ru.js'].imports.push('lazy-extra.js');
  bundle['index.html'].source +=
    '<link href="/desktop/lazy-extra.js?generated=1" rel="modulepreload">';

  assert.throws(
    () => assertI18nBundleBoundaries(bundle),
    /Locale graph ru was preloaded by index.html: lazy-extra.js/,
  );
});

for (const [kind, foreignModule] of [
  ['pack', '/ui/src/shared/i18n/packs/fr.ts'],
  ['catalog', '/ui/src/shared/i18n/messages/fr.ts'],
  ['Luma override', '/ui/src/shared/i18n/messages/overrides/luma/fr.ts'],
  ['NVAPI override', '/ui/src/shared/i18n/messages/overrides/nvapi/fr.ts'],
]) {
  test(`rejects a foreign ${kind} in an otherwise lazy locale graph`, () => {
    const bundle = createBundle();
    bundle['ru-cross-locale.js'] = chunk('ru-cross-locale.js', {
      modules: { [foreignModule]: {} },
    });
    bundle['ru.js'].imports.push('ru-cross-locale.js');

    assert.throws(
      () => assertI18nBundleBoundaries(bundle),
      /Locale graph ru imports modules owned by another locale/,
    );
  });
}

for (const [owner, foreignLocale] of [
  ['zh-Hans', 'zh-Hant'],
  ['zh-Hant', 'zh-Hans'],
]) {
  test(`distinguishes ${owner} from the similarly named ${foreignLocale} graph`, () => {
    const bundle = createBundle();
    const crossLocaleChunk = `${owner}-cross-locale.js`;
    bundle[crossLocaleChunk] = chunk(crossLocaleChunk, {
      modules: { [`/ui/src/shared/i18n/messages/${foreignLocale}.ts`]: {} },
    });
    bundle[`${owner}.js`].imports.push(crossLocaleChunk);

    assert.throws(
      () => assertI18nBundleBoundaries(bundle),
      new RegExp(`Locale graph ${owner} imports modules owned by another locale`),
    );
  });
}
