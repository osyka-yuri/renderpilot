import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { ESLint } from 'eslint';
import boundaries from 'eslint-plugin-boundaries';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createFsdBoundariesConfig,
  FSD_ALIAS_PREFIXES,
  FSD_SLICED_LAYERS,
} from './fsd-boundaries.js';

const PROJECT_ROOT = path.resolve(import.meta.dirname, '..');
const ESLINT_CONFIG_FILE = path.join(PROJECT_ROOT, 'eslint.config.js');
const TEMP_PREFIX = path.join(os.tmpdir(), 'renderpilot-fsd-boundaries-');
const TEMP_SOURCE_ROOT = 'ui/src';
const FACTORY_EXTENSIONS = ['js', 'ts'];
const FACTORY_TARGET_EXTENSIONS = [...FACTORY_EXTENSIONS, 'css'];
const FACTORY_RESOLVER_EXTENSIONS = ['.js', '.ts', '.css'];

let tempRoot;
let tempLinter;

function tempPath(relativePath) {
  return path.join(tempRoot, relativePath);
}

async function writeFixture(relativePath, content = 'export {};\n') {
  const filePath = tempPath(relativePath);

  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, content);
}

function createFactoryConfig(rootPath, extensions = {}) {
  const {
    publicApiExtensions = FACTORY_EXTENSIONS,
    targetExtensions = FACTORY_TARGET_EXTENSIONS,
    resolverExtensions = FACTORY_RESOLVER_EXTENSIONS,
  } = extensions;

  return createFsdBoundariesConfig({
    rootPath,
    sourceRoot: TEMP_SOURCE_ROOT,
    publicApiExtensions,
    targetExtensions,
    resolverExtensions,
    typescriptConfigPath: path.join(rootPath, 'tsconfig.json'),
  });
}

async function createTempFixtures() {
  await writeFixture(
    'tsconfig.json',
    JSON.stringify({
      compilerOptions: {
        allowJs: true,
        baseUrl: '.',
        module: 'ESNext',
        moduleResolution: 'Bundler',
        paths: {
          '@app/*': ['./ui/src/app/*'],
          '@pages/*': ['./ui/src/pages/*'],
          '@widgets/*': ['./ui/src/widgets/*'],
          '@features/*': ['./ui/src/features/*'],
          '@entities/*': ['./ui/src/entities/*'],
          '@shared/*': ['./ui/src/shared/*'],
        },
      },
    }),
  );

  await Promise.all(
    [
      'ui/src/app/bootstrap.ts',
      'ui/src/main.ts',
      'ui/src/pages/home/index.ts',
      'ui/src/widgets/settings/index.ts',
      'ui/src/widgets/settings/ui/private.ts',
      'ui/src/features/alpha/index.ts',
      'ui/src/features/alpha/model/private.ts',
      'ui/src/features/alpha/model/local.ts',
      'ui/src/features/beta/model/private.ts',
      'ui/src/entities/user/index.ts',
      'ui/src/entities/user/model/private.ts',
      'ui/src/shared/theme/index.ts',
      'ui/src/shared/theme/index.css',
      'ui/src/shared/theme/global.css',
      'ui/src/shared/theme/theme-mode.ts',
      'ui/src/shared/theme/tokens.ts',
      'ui/src/shared/i18n/index.ts',
      'ui/src/shared/i18n/core.ts',
      'ui/src/unclassified.ts',
      'node_modules/example-package/index.js',
    ].map((fixture) => writeFixture(fixture, fixture.endsWith('.css') ? ':root {}\n' : undefined)),
  );
}

function createTempLinter(config = createFactoryConfig(tempRoot)) {
  return new ESLint({
    cwd: tempRoot,
    overrideConfigFile: true,
    overrideConfig: [
      {
        files: ['**/*.{js,ts}'],
        plugins: {
          boundaries,
        },
        settings: config.settings,
        rules: config.rules,
      },
    ],
  });
}

async function lintAt(relativePath, code) {
  await writeFixture(relativePath, code);

  return tempLinter.lintText(code, {
    filePath: tempPath(relativePath),
  });
}

function ruleIds(result) {
  return result[0].messages.map((message) => message.ruleId);
}

function expectNoMessages(result) {
  expect(result[0].messages).toEqual([]);
}

function expectRule(result, ruleId) {
  expect(ruleIds(result)).toContain(ruleId);
}

function ruleSeverity(config, ruleId) {
  const value = config.rules[ruleId];

  return Array.isArray(value) ? value[0] : value;
}

function expectBoundariesEnabled(config) {
  expect(ruleSeverity(config, 'boundaries/no-unknown-files')).toBe(2);
  expect(ruleSeverity(config, 'boundaries/no-unknown-dependencies')).toBe(2);
  expect(ruleSeverity(config, 'boundaries/no-ignored-dependencies')).toBe(2);
  expect(ruleSeverity(config, 'boundaries/dependencies')).toBe(2);
  expect(config.settings['boundaries/elements-single-match']).toBe(true);
  expect(config.settings['boundaries/files-single-match']).toBe(true);
  expect(config.settings['boundaries/legacy-templates']).toBe(false);
  expect(config.settings['boundaries/legacy-warnings']).toBe(false);
}

beforeEach(async () => {
  tempRoot = await mkdtemp(TEMP_PREFIX);
  await createTempFixtures();
  tempLinter = createTempLinter();
});

afterEach(async () => {
  const rootToRemove = tempRoot;

  tempLinter = undefined;
  tempRoot = undefined;

  if (rootToRemove !== undefined) {
    await rm(rootToRemove, { recursive: true, force: true });
  }
});

describe('FSD boundaries factory', () => {
  it.each([
    {
      name: 'allows same-slice internals',
      source: 'ui/src/features/alpha/model/source.ts',
      code: "import './local';\n",
      expectedRule: undefined,
    },
    {
      name: 'classifies root-level slice internals',
      source: 'ui/src/features/alpha/root-level.ts',
      code: "import '@entities/user';\n",
      expectedRule: undefined,
    },
    {
      name: 'classifies nested index files as slice internals',
      source: 'ui/src/features/alpha/model/index.ts',
      code: "import '@entities/user';\n",
      expectedRule: undefined,
    },
    {
      name: 'denies sibling-slice internals',
      source: 'ui/src/features/alpha/model/source.ts',
      code: "import '@features/beta/model/private';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'allows higher layers to import lower public APIs',
      source: 'ui/src/pages/home/ui/source.ts',
      code: "import '@widgets/settings';\n",
      expectedRule: undefined,
    },
    {
      name: 'denies higher layers importing lower internals',
      source: 'ui/src/pages/home/ui/source.ts',
      code: "import '@widgets/settings/ui/private';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'denies lower layers importing higher public APIs',
      source: 'ui/src/entities/user/model/source.ts',
      code: "import '@features/alpha';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'allows public facades to import same-slice internals',
      source: 'ui/src/features/alpha/index.ts',
      code: "import './model/private';\n",
      expectedRule: undefined,
    },
    {
      name: 'denies public facades importing lower layers',
      source: 'ui/src/features/alpha/index.ts',
      code: "import '@entities/user';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'denies internals importing their own public facade',
      source: 'ui/src/features/alpha/model/source.ts',
      code: "import '../index';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'allows shared internals in the same segment',
      source: 'ui/src/shared/theme/theme-mode.ts',
      code: "import './tokens';\n",
      expectedRule: undefined,
    },
    {
      name: 'allows shared internals to import other shared public APIs',
      source: 'ui/src/shared/theme/theme-mode.ts',
      code: "import '@shared/i18n';\n",
      expectedRule: undefined,
    },
    {
      name: 'denies shared internals importing another segment internals',
      source: 'ui/src/shared/theme/theme-mode.ts',
      code: "import '@shared/i18n/core';\n",
      expectedRule: 'boundaries/dependencies',
    },
    {
      name: 'allows app internals to import page and shared public APIs',
      source: 'ui/src/app/bootstrap.ts',
      code: "import '@pages/home';\nimport '@shared/theme';\n",
      expectedRule: undefined,
    },
    {
      name: 'allows external package imports',
      source: 'ui/src/app/bootstrap.ts',
      code: "import 'example-package';\n",
      expectedRule: undefined,
    },
    {
      name: 'allows shared public theme facades to import their CSS internals',
      source: 'ui/src/shared/theme/index.ts',
      code: "import './global.css';\n",
      expectedRule: undefined,
    },
    {
      name: 'classifies root CSS index files as shared internals',
      source: 'ui/src/shared/theme/index.ts',
      code: "import './index.css';\n",
      expectedRule: undefined,
    },
    {
      name: 'classifies root index test files as shared internals',
      source: 'ui/src/shared/theme/index.test.ts',
      code: "import './index';\n",
      expectedRule: undefined,
    },
  ])('$name', async ({ source, code, expectedRule }) => {
    const result = await lintAt(source, code);

    if (expectedRule === undefined) {
      expectNoMessages(result);
    } else {
      expectRule(result, expectedRule);
    }
  });

  it('reports unknown targets, unknown sources, and ignored dependencies with native rules', async () => {
    const unknownTarget = await lintAt(
      'ui/src/app/unknown-target-importer.ts',
      "import '../unclassified';\n",
    );
    const unknownSource = await lintAt('ui/src/unclassified.ts', 'export {};\n');
    const ignoredTarget = await lintAt(
      'ui/src/app/ignored-target-importer.ts',
      "import '../main';\n",
    );

    expectRule(unknownTarget, 'boundaries/no-unknown-dependencies');
    expectRule(unknownSource, 'boundaries/no-unknown-files');
    expectRule(ignoredTarget, 'boundaries/no-ignored-dependencies');
  });

  it('keeps multi-dot public facades public when descriptors are reordered internal-first', async () => {
    const config = createFactoryConfig(tempRoot, {
      publicApiExtensions: ['ts', 'svelte.ts'],
      targetExtensions: ['ts', 'svelte.ts', 'css'],
      resolverExtensions: ['.ts', '.svelte.ts', '.css'],
    });

    config.settings['boundaries/files'].reverse();

    const linter = createTempLinter(config);
    const result = await linter.lintText("import '@entities/user';\n", {
      filePath: tempPath('ui/src/features/alpha/index.svelte.ts'),
    });

    expectRule(result, 'boundaries/dependencies');
  });

  it('keeps public facade classification when file descriptors are reordered internal-first', async () => {
    const config = createFactoryConfig(tempRoot);

    config.settings['boundaries/files'].reverse();

    const linter = createTempLinter(config);
    const result = await linter.lintText("import '@entities/user';\n", {
      filePath: tempPath('ui/src/features/alpha/index.ts'),
    });

    expectRule(result, 'boundaries/dependencies');
  });

  it('uses modern descriptors, entity policies, and warning-free v7 settings', async () => {
    const config = createFactoryConfig(tempRoot);
    const { settings, rules } = config;
    const elements = settings['boundaries/elements'];
    const files = settings['boundaries/files'];
    const dependencies = rules['boundaries/dependencies'][1];
    const policies = dependencies.policies;

    expect(FSD_SLICED_LAYERS).toEqual(['pages', 'widgets', 'features', 'entities']);
    expect(FSD_ALIAS_PREFIXES).toEqual([
      '@/pages',
      '@/widgets',
      '@/features',
      '@/entities',
      '@/shared',
      '@pages',
      '@widgets',
      '@features',
      '@entities',
      '@shared',
    ]);
    expect(config.entryPointGlobs).toEqual([
      'ui/src/main.{js,ts}',
      'ui/src/App.svelte',
      'ui/src/app.d.ts',
      'ui/src/vite-env.d.ts',
    ]);
    expect(settings).not.toHaveProperty('boundaries/include');
    const publicApiFileDescriptors = files.slice(0, 5);
    const internalFileDescriptors = files.slice(5);
    const descriptorPatterns = files.flatMap(({ pattern }) =>
      Array.isArray(pattern) ? pattern : [pattern],
    );

    expect(Object.isFrozen(FSD_SLICED_LAYERS)).toBe(true);
    expect(Object.isFrozen(FSD_ALIAS_PREFIXES)).toBe(true);
    expect(elements).toHaveLength(6);
    expect(elements.every((element) => element.partialMatch === false)).toBe(true);
    expect(elements.every((element) => !('category' in element) && !('mode' in element))).toBe(
      true,
    );
    expect(settings['boundaries/elements-single-match']).toBe(true);
    expect(settings['boundaries/files-single-match']).toBe(true);
    expect(publicApiFileDescriptors.every((file) => file.category === 'public-api')).toBe(true);
    expect(internalFileDescriptors.every((file) => file.category === 'internal')).toBe(true);
    expect(internalFileDescriptors.slice(1).every((file) => Array.isArray(file.pattern))).toBe(
      true,
    );
    expect(
      internalFileDescriptors
        .slice(1)
        .every((file) =>
          file.pattern.some(
            (pattern) =>
              pattern.includes('/!(index.js|index.ts)@(*.{js,ts,css})') &&
              pattern.endsWith('@(*.{js,ts,css})'),
          ),
        ),
    ).toBe(true);
    expect(descriptorPatterns.every((pattern) => !pattern.startsWith('ui/src/**/*'))).toBe(true);
    expect(Object.keys(rules).sort()).toEqual([
      'boundaries/dependencies',
      'boundaries/no-ignored-dependencies',
      'boundaries/no-unknown-dependencies',
      'boundaries/no-unknown-files',
    ]);
    expect(rules['boundaries/no-unknown-dependencies']).toEqual(['error', { require: 'all' }]);
    expect(dependencies).toHaveProperty('policies');
    expect(dependencies).not.toHaveProperty('rules');
    expect(dependencies.message).toContain('{{ from.element.types.[0] }}');
    expect(dependencies.message).toContain('{{ to.element.types.[0] }}');
    expect(JSON.stringify(config)).not.toContain('from.captured');
    expect(JSON.stringify(config)).not.toContain('from.type');

    for (const policy of policies) {
      expect(Object.keys(policy).sort()).toEqual(['allow', 'from']);
      expect(Object.keys(policy.from).sort()).toEqual(['element', 'file']);
      expect(policy.from.element.types).toHaveLength(1);
      expect(policy.from.file.categories).toHaveLength(1);
      expect(Array.isArray(policy.allow)).toBe(true);

      for (const target of policy.allow) {
        expect(Object.keys(target)).toEqual(['to']);
        expect(Object.keys(target.to).sort()).toEqual(['element', 'file']);
        expect(target.to.element.types).toHaveLength(1);
        expect(target.to.file.categories).toHaveLength(1);
      }
    }

    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const originalEmitWarning = process.emitWarning;
    const processWarnings = [];

    process.emitWarning = (...warning) => {
      processWarnings.push(warning);
    };

    try {
      const result = await lintAt(
        'ui/src/features/alpha/model/warning-check.ts',
        "import './local';\n",
      );
      const boundaryWarnings = [...consoleWarn.mock.calls, ...processWarnings]
        .flat()
        .map((warning) => String(warning))
        .filter((warning) => /boundaries/i.test(warning));

      expectNoMessages(result);
      expect(boundaryWarnings.some((warning) => /legacy|deprecat/i.test(warning))).toBe(false);
    } finally {
      process.emitWarning = originalEmitWarning;
      consoleWarn.mockRestore();
    }
  });

  it('isolates nested selectors and resolver extensions between factory results', () => {
    const first = createFactoryConfig(tempRoot);
    const second = createFactoryConfig(tempRoot);
    const firstSharedPublicPolicy = first.rules['boundaries/dependencies'][1].policies.find(
      (policy) =>
        policy.from.element.types[0] === 'shared' &&
        policy.from.file.categories[0] === 'public-api',
    );
    const secondSharedPublicPolicy = second.rules['boundaries/dependencies'][1].policies.find(
      (policy) =>
        policy.from.element.types[0] === 'shared' &&
        policy.from.file.categories[0] === 'public-api',
    );

    firstSharedPublicPolicy.allow[0].to.element.captured.segment = 'mutated';
    first.settings['import/resolver'].node.extensions.push('.mutated');

    const third = createFactoryConfig(tempRoot);
    const thirdSharedPublicPolicy = third.rules['boundaries/dependencies'][1].policies.find(
      (policy) =>
        policy.from.element.types[0] === 'shared' &&
        policy.from.file.categories[0] === 'public-api',
    );

    expect(secondSharedPublicPolicy.allow[0].to.element.captured.segment).toBe(
      '{{ from.element.captured.segment }}',
    );
    expect(thirdSharedPublicPolicy.allow[0].to.element.captured.segment).toBe(
      '{{ from.element.captured.segment }}',
    );
    expect(second.settings['import/resolver'].node.extensions).not.toContain('.mutated');
    expect(third.settings['import/resolver'].node.extensions).not.toContain('.mutated');
  });
});

describe('production FSD composition', () => {
  it('composes boundaries and import policies for representative source paths', async () => {
    const eslint = new ESLint({
      cwd: PROJECT_ROOT,
      overrideConfigFile: ESLINT_CONFIG_FILE,
    });

    const representativeConfigs = await Promise.all(
      [
        ['JavaScript source', 'ui/src/features/example/probe.js'],
        ['TypeScript source', 'ui/src/features/sync-covers/model/notifications.ts'],
        ['Svelte source', 'ui/src/features/filter-games/ui/GamesFilterDialog.svelte'],
        ['shared foundation source', 'ui/src/shared/theme/index.ts'],
        ['app bootstrap source', 'ui/src/app/bootstrap.ts'],
        ['foundation entry point', 'ui/src/main.ts'],
      ].map(async ([label, relativePath]) => [
        label,
        await eslint.calculateConfigForFile(path.join(PROJECT_ROOT, relativePath)),
      ]),
    );

    for (const [label, config] of representativeConfigs) {
      expect(config, label).toBeDefined();
      expect(ruleSeverity(config, 'local-architecture/import-boundaries'), label).toBe(2);
    }

    for (const [label, config] of representativeConfigs.slice(0, 5)) {
      expectBoundariesEnabled(config, label);
    }

    const javascriptConfig = representativeConfigs[0][1];
    const typeScriptConfig = representativeConfigs[1][1];
    const svelteConfig = representativeConfigs[2][1];
    expect(ruleSeverity(javascriptConfig, 'no-restricted-imports')).toBe(2);
    expect(ruleSeverity(typeScriptConfig, '@typescript-eslint/no-restricted-imports')).toBe(2);
    expect(ruleSeverity(svelteConfig, '@typescript-eslint/no-restricted-imports')).toBe(2);
    expect(typeScriptConfig.rules['@typescript-eslint/no-restricted-imports']).toEqual(
      javascriptConfig.rules['no-restricted-imports'],
    );
    expect(svelteConfig.rules['@typescript-eslint/no-restricted-imports']).toEqual(
      javascriptConfig.rules['no-restricted-imports'],
    );
    expect(javascriptConfig.rules['@typescript-eslint/no-restricted-imports']).toBeUndefined();
    expect(ruleSeverity(typeScriptConfig, 'no-restricted-imports')).toBe(0);
    expect(ruleSeverity(svelteConfig, 'no-restricted-imports')).toBe(0);

    const [javascriptResult] = await eslint.lintText(
      "import '@shared';\nimport '@shared/i18n/private';\n",
      {
        filePath: path.join(PROJECT_ROOT, 'ui/src/features/example/probe.js'),
      },
    );
    const restrictedImportMessages = javascriptResult.messages.filter(
      (message) => message.ruleId === 'no-restricted-imports',
    );
    expect(restrictedImportMessages).toHaveLength(2);
    expect(
      restrictedImportMessages.some((message) => message.message.includes('shared layer root')),
    ).toBe(true);
    expect(
      restrictedImportMessages.some((message) =>
        message.message.includes('shared segment public API'),
      ),
    ).toBe(true);

    const foundationConfig = representativeConfigs.at(-1)[1];
    expect(foundationConfig.rules['boundaries/no-unknown-files']).toBeUndefined();
    expect(ruleSeverity(foundationConfig, 'no-restricted-imports')).toBe(0);
    expect(ruleSeverity(foundationConfig, '@typescript-eslint/no-restricted-imports')).toBe(0);
    expect(ruleSeverity(foundationConfig, 'local-architecture/no-fsd-alias-re-export')).toBe(2);
    expect(ruleSeverity(foundationConfig, 'local-architecture/import-boundaries')).toBe(2);
  }, 15_000);
});
