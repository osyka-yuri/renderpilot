import path from 'node:path';

import { RuleTester } from 'eslint';
import { describe, it } from 'vitest';
import svelteParser from 'svelte-eslint-parser';
import tseslint from 'typescript-eslint';

import { createImportBoundariesRule } from './import-boundaries.js';

const projectRoot = path.resolve(import.meta.dirname, '..');
const foundationEntryPoints = ['ui/src/main.{js,ts}'];

function filename(relativePath) {
  return path.join(projectRoot, relativePath);
}

const typeScriptLanguageOptions = {
  parser: tseslint.parser,
  ecmaVersion: 'latest',
  sourceType: 'module',
};

const svelteTypeScriptLanguageOptions = {
  parser: svelteParser,
  ecmaVersion: 'latest',
  sourceType: 'module',
  parserOptions: {
    parser: tseslint.parser,
  },
};

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
});

ruleTester.run(
  'import-boundaries',
  createImportBoundariesRule({
    projectRoot,
    foundationEntryPoints,
  }),
  {
    valid: [
      {
        name: 'does not guess unknown or shadowed Bits UI imports',
        code: 'void import(prefix);',
        filename: filename('ui/src/features/example/probe.js'),
      },
      {
        name: 'does not guess template prefixes without a package separator',
        code: 'void import(`bits-ui${suffix}`);',
        filename: filename('ui/src/features/example/probe.js'),
      },
      {
        name: 'does not guess unknown conditional branches',
        code: 'void import(flag ? unknown : other);',
        filename: filename('ui/src/features/example/probe.js'),
      },
      {
        name: 'ignores shadowed CommonJS require for Bits UI',
        code: "function require(value) { return value; }\nvoid require('bits-ui');",
        filename: filename('ui/src/features/example/probe.js'),
      },
      {
        name: 'allows Bits UI in shared UI ownership surface',
        code: "import { Menu } from 'bits-ui';",
        filename: filename('ui/src/shared/ui/example.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows static FSD imports for native restriction ownership',
        code: "import '@shared';",
        filename: filename('ui/src/features/example/probe.js'),
      },
      {
        name: 'allows static TypeScript import-equals for native restriction ownership',
        code: "import Feature = require('@features/filter-games/ui/private');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows legal dynamic FSD public APIs',
        code: "void import('@features/filter-games');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows legal aliased dynamic FSD public APIs',
        code: "void import('@/shared/i18n');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows dynamic FSD paths with unknown slice names',
        code: 'void import(`@features/${slice}`);',
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows dynamic shared public APIs',
        code: "void require('@shared/i18n');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'ignores shadowed CommonJS require for FSD imports',
        code: "function require(value) { return value; }\nvoid require('@shared');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows dynamic FSD imports from a foundation entry point',
        code: "void import('@shared');\nvoid require('@shared');\nvoid require('@shared/i18n/private');",
        filename: filename('ui/src/main.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows Bits UI in its Svelte shared UI owner surface',
        code: "<script>import { Menu } from 'bits-ui';</script>",
        filename: filename('ui/src/shared/ui/alert/alert.svelte'),
        languageOptions: svelteTypeScriptLanguageOptions,
      },
      {
        name: 'allows Sonner in its Svelte owner surface',
        code: "<script>import { Toaster } from 'svelte-sonner';</script>",
        filename: filename('ui/src/shared/ui/sonner/sonner.svelte'),
        languageOptions: svelteTypeScriptLanguageOptions,
      },
      {
        name: 'allows Sonner in the notification adapter',
        code: "import { toast } from 'svelte-sonner';",
        filename: filename('ui/src/widgets/notifications-toaster/notification-adapter.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
      {
        name: 'allows Sonner in the notification adapter test',
        code: "import { toast } from 'svelte-sonner';",
        filename: filename('ui/src/widgets/notifications-toaster/notification-adapter.test.ts'),
        languageOptions: typeScriptLanguageOptions,
      },
    ],
    invalid: [
      {
        name: 'reports direct Bits UI imports',
        code: "import { Menu } from 'bits-ui';",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports Bits UI named re-exports',
        code: "export { Menu } from 'bits-ui/dropdown-menu';",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports Bits UI star re-exports',
        code: "export * from 'bits-ui/internal/menu';",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports dynamic Bits UI imports',
        code: "void import('bits-ui');",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports dynamic Bits UI template imports',
        code: 'void import(`bits-ui/${suffix}`);',
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports dynamic Bits UI concatenation',
        code: "void import(`${'bits-ui'}${`/${suffix}`}`);",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports CommonJS Bits UI concatenation',
        code: "void require('bits-ui' + ('/' + suffix));",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports conditional Bits UI imports',
        code: "void import(flag ? 'bits-ui/foo' : 'bits-ui/bar');",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports conditional Bits UI imports with an unknown branch',
        code: "void import(flag ? 'bits-ui/foo' : unknown);",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports logical Bits UI imports with an unknown branch',
        code: "void import(unknown && 'bits-ui/foo');",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports conditional CommonJS Bits UI imports with an unknown branch',
        code: "void require(flag ? 'bits-ui/foo' : unknown);",
        filename: filename('ui/src/features/example/probe.js'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports direct Bits UI TypeScript import-equals syntax',
        code: "import BitsUi = require('bits-ui');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports Bits UI imports outside its owner surface',
        code: "import { Menu } from 'bits-ui';",
        filename: filename('ui/src/main.ts'),
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports dynamic FSD roots',
        code: "void import('@shared');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports dynamic FSD internal paths',
        code: "void import('@shared/i18n/private');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports dynamic FSD template paths',
        code: 'void import(`@shared/i18n/${suffix}`);',
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports conditional dynamic FSD roots',
        code: "void import(flag ? '@shared' : unknown);",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports CommonJS FSD internal paths',
        code: "void require('@features/filter-games/ui/private');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports CommonJS FSD roots',
        code: "void require('@shared');",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports conditional CommonJS FSD internal paths',
        code: "void require(flag ? '@shared/i18n/private' : unknown);",
        filename: filename('ui/src/features/example/probe.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
      {
        name: 'reports direct Sonner dispatch imports',
        code: "import { toast } from 'svelte-sonner';",
        filename: filename('ui/src/pages/example/index.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'directSonnerImport' }],
      },
      {
        name: 'reports dynamic Sonner dispatch imports',
        code: "void import('svelte-sonner');",
        filename: filename('ui/src/pages/example/index.ts'),
        languageOptions: typeScriptLanguageOptions,
        errors: [{ messageId: 'directSonnerImport' }],
      },
      {
        name: 'reports direct Bits UI imports in product Svelte',
        code: "<script>import { Menu } from 'bits-ui';</script>",
        filename: filename('ui/src/features/example/Example.svelte'),
        languageOptions: svelteTypeScriptLanguageOptions,
        errors: [{ messageId: 'directBitsUiImport' }],
      },
      {
        name: 'reports dynamic FSD imports in product Svelte',
        code: '<script lang="ts">void import(\'@shared\');</script>',
        filename: filename('ui/src/features/example/Example.svelte'),
        languageOptions: svelteTypeScriptLanguageOptions,
        errors: [{ messageId: 'restrictedFsdImport' }],
      },
    ],
  },
);
