import path from 'node:path';

import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import eslintConfigPrettier from 'eslint-config-prettier';
import betterTailwindcss from 'eslint-plugin-better-tailwindcss';
import { getDefaultSelectors } from 'eslint-plugin-better-tailwindcss/defaults';
import boundaries from 'eslint-plugin-boundaries';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import tseslint from 'typescript-eslint';

import {
  createFsdBoundariesConfig,
  FSD_ALIAS_PREFIXES,
  FSD_SLICED_LAYERS,
} from './eslint/fsd-boundaries.js';
import { createIntlBoundariesRule } from './eslint/intl-boundaries.js';
import svelteConfig from './svelte.config.js';

const PROJECT_ROOT = import.meta.dirname;

const SOURCE_ROOT = 'ui/src';
const TAILWIND_ENTRY_POINT = `${SOURCE_ROOT}/shared/theme/global.css`;
const TYPESCRIPT_CONFIG = './tsconfig.json';
const intlBoundariesRule = createIntlBoundariesRule({
  projectRoot: PROJECT_ROOT,
  sourceRoot: SOURCE_ROOT,
});

const JAVASCRIPT_EXTENSIONS = ['js', 'jsx', 'mjs', 'cjs'];
const TYPESCRIPT_EXTENSIONS = ['ts', 'tsx', 'mts', 'cts'];
const SVELTE_EXTENSIONS = ['svelte'];
const SVELTE_MODULE_EXTENSIONS = ['svelte.js', 'svelte.ts'];

const SOURCE_SCRIPT_EXTENSIONS = [
  ...JAVASCRIPT_EXTENSIONS,
  ...TYPESCRIPT_EXTENSIONS,
  ...SVELTE_EXTENSIONS,
];

const RESOLVER_EXTENSIONS = [
  ...SOURCE_SCRIPT_EXTENSIONS.map((extension) => `.${extension}`),
  ...SVELTE_MODULE_EXTENSIONS.map((extension) => `.${extension}`),
  '.css',
];

const GLOBAL_IGNORES = [
  '**/node_modules/**',
  '**/dist/**',
  '**/build/**',
  '**/coverage/**',
  '**/.svelte-kit/**',

  '**/src-tauri/target/**',
  '**/src-tauri/gen/**',

  '**/pnpm-lock.yaml',

  /*
   * CSS is not parsed as an ESLint source file.
   * It is still included in boundaries target globs so style imports can be
   * resolved and classified by eslint-plugin-boundaries.
   */
  '**/*.css',

  'eslint.config.js',
  'oxfmt.config.{js,cjs,mjs,ts,mts,cts}',
  'svelte.config.{js,cjs,mjs,ts}',
  'tailwind.config.{js,cjs,mjs,ts}',
  'vite.config.{js,cjs,mjs,ts}',
  'vitest.config.{js,cjs,mjs,ts}',
];

const UNUSED_VALUE_OPTIONS = {
  args: 'after-used',
  argsIgnorePattern: '^_',
  varsIgnorePattern: '^_',
  caughtErrors: 'all',
  caughtErrorsIgnorePattern: '^_',
  destructuredArrayIgnorePattern: '^_',
  ignoreRestSiblings: true,
};

function toBraceGlob(values) {
  return values.join(',');
}

function sourceFiles(extensions) {
  return [`${SOURCE_ROOT}/**/*.{${toBraceGlob(extensions)}}`];
}

function scopeConfigs(configs, files) {
  return configs.map((config) => ({
    ...config,
    files,
  }));
}

const SOURCE_FILE_GLOBS = sourceFiles(SOURCE_SCRIPT_EXTENSIONS);
const JAVASCRIPT_FILE_GLOBS = sourceFiles(JAVASCRIPT_EXTENSIONS);
const TYPESCRIPT_FILE_GLOBS = sourceFiles(TYPESCRIPT_EXTENSIONS);
const TOOLING_JAVASCRIPT_FILE_GLOBS = ['eslint/**/*.js', 'scripts/**/*.mjs'];

const SVELTE_COMPONENT_FILE_GLOBS = [`${SOURCE_ROOT}/**/*.svelte`];
const SVELTE_TYPESCRIPT_MODULE_FILE_GLOBS = [`${SOURCE_ROOT}/**/*.svelte.ts`];
const SVELTE_MODULE_FILE_GLOBS = [
  `${SOURCE_ROOT}/**/*.svelte.js`,
  ...SVELTE_TYPESCRIPT_MODULE_FILE_GLOBS,
];
const SVELTE_FILE_GLOBS = [...SVELTE_COMPONENT_FILE_GLOBS, ...SVELTE_MODULE_FILE_GLOBS];

/*
 * Lint ownership:
 * - Oxlint owns ordinary TypeScript in the application and Node tooling,
 *   including its type-aware rules.
 * - ESLint owns JavaScript tooling with Node runtime globals.
 * - ESLint also owns Svelte components and Svelte TypeScript modules because
 *   it understands templates and compiler semantics such as runes.
 * - ESLint also owns project-specific architecture, Tailwind, and import
 *   rules that cannot be expressed faithfully in Oxlint.
 */
const SVELTE_TYPE_AWARE_FILE_GLOBS = [
  ...SVELTE_COMPONENT_FILE_GLOBS,
  ...SVELTE_TYPESCRIPT_MODULE_FILE_GLOBS,
];
const ESLINT_BASE_FILE_GLOBS = [...JAVASCRIPT_FILE_GLOBS, ...SVELTE_TYPE_AWARE_FILE_GLOBS];

const TEST_FILE_GLOBS = [
  `${SOURCE_ROOT}/**/*.{test,spec}.{${toBraceGlob(SOURCE_SCRIPT_EXTENSIONS)}}`,
];

const fsdBoundariesConfig = createFsdBoundariesConfig({
  rootPath: PROJECT_ROOT,
  sourceRoot: SOURCE_ROOT,
  publicApiExtensions: [...SOURCE_SCRIPT_EXTENSIONS, ...SVELTE_MODULE_EXTENSIONS],
  targetExtensions: [...SOURCE_SCRIPT_EXTENSIONS, ...SVELTE_MODULE_EXTENSIONS, 'css'],
  resolverExtensions: RESOLVER_EXTENSIONS,
  typescriptConfigPath: path.resolve(PROJECT_ROOT, TYPESCRIPT_CONFIG),
});

const eslintBaseConfigs = scopeConfigs(
  [js.configs.recommended],
  [...ESLINT_BASE_FILE_GLOBS, ...TOOLING_JAVASCRIPT_FILE_GLOBS],
);

const typescriptParserConfigs = scopeConfigs([tseslint.configs.base], TYPESCRIPT_FILE_GLOBS);

const typeCheckedTypeScriptConfigs = scopeConfigs(
  [...tseslint.configs.strictTypeChecked, ...tseslint.configs.stylisticTypeChecked],
  SVELTE_TYPE_AWARE_FILE_GLOBS,
);

/*
 * CVA shape:
 *
 * cva('base classes', {
 *   variants: {
 *     intent: {
 *       primary: '...',
 *       secondary: '...',
 *     },
 *   },
 *   compoundVariants: [
 *     {
 *       intent: 'primary',
 *       class: '...',
 *     },
 *     {
 *       intent: 'secondary',
 *       className: '...',
 *     },
 *   ],
 * })
 *
 * We intentionally do not lint every object value inside cva().
 * Values in defaultVariants and variant selectors are not Tailwind classes.
 */
const CVA_VARIANT_CLASS_VALUE_PATH =
  '^variants(?:\\.[\\w$-]+|\\["[^"]+"\\])(?:\\.[\\w$-]+|\\["[^"]+"\\])$';

const CVA_COMPOUND_VARIANT_CLASS_VALUE_PATH = '^compoundVariants\\[\\d+\\]\\.(?:class|className)$';

function createClassCompositionSelector(name) {
  return {
    kind: 'callee',
    name,
    match: [
      {
        type: 'strings',
      },
      {
        type: 'objectKeys',
      },
    ],
  };
}

const tailwindClassSelectors = [
  ...getDefaultSelectors(),

  /*
   * Common class composition helpers:
   *
   * cn('flex items-center', condition && 'opacity-50', {
   *   'pointer-events-none opacity-50': disabled,
   * })
   */
  createClassCompositionSelector('^cn$'),

  /*
   * Same behavior for clsx/cx if they are used directly.
   */
  createClassCompositionSelector('^(?:clsx|cx)$'),

  /*
   * class-variance-authority.
   *
   * `strings` covers base classes: cva('...')
   * `objectValues + variants path` covers variants.intent.primary = '...'
   * `objectValues + compoundVariants path` covers compoundVariants[].class/className
   */
  {
    kind: 'callee',
    name: '^cva$',
    match: [
      {
        type: 'strings',
      },
      {
        type: 'objectValues',
        path: CVA_VARIANT_CLASS_VALUE_PATH,
      },
      {
        type: 'objectValues',
        path: CVA_COMPOUND_VARIANT_CLASS_VALUE_PATH,
      },
    ],
  },
];

function createLayerRootAliasRestrictions(layer) {
  return [
    {
      name: `@/${layer}`,
      message:
        `Do not import the "${layer}" FSD layer root directly. ` +
        `Import a concrete slice public API: "@/${layer}/<slice>" or "@${layer}/<slice>".`,
    },
    {
      name: `@${layer}`,
      message:
        `Do not import the "${layer}" FSD layer root directly. ` +
        `Import a concrete slice public API: "@/${layer}/<slice>" or "@${layer}/<slice>".`,
    },
  ];
}

function createSlicedLayerDeepImportPatterns(layer) {
  return [`@/${layer}/*/*`, `@/${layer}/*/*/**`, `@${layer}/*/*`, `@${layer}/*/*/**`];
}

const restrictedFsdRootImports = [
  ...FSD_SLICED_LAYERS.flatMap(createLayerRootAliasRestrictions),

  {
    name: '@/shared',
    message:
      'Do not import the shared layer root directly. Import a concrete shared segment public API: "@/shared/<segment>" or "@shared/<segment>".',
  },
  {
    name: '@shared',
    message:
      'Do not import the shared layer root directly. Import a concrete shared segment public API: "@/shared/<segment>" or "@shared/<segment>".',
  },
];

const restrictedFsdDeepImportPatterns = [
  {
    group: FSD_SLICED_LAYERS.flatMap(createSlicedLayerDeepImportPatterns),
    message:
      'Import FSD slices only through their public API: "@/features/<slice>" / "@features/<slice>". Internal slice files must use relative imports inside the same slice.',
  },
  {
    group: ['@/shared/*/*', '@/shared/*/*/**', '@shared/*/*', '@shared/*/*/**'],
    message:
      'Import shared code through a segment public API: "@/shared/<segment>" / "@shared/<segment>". Internal shared files must use relative imports inside the same shared segment.',
  },
];

const fsdRestrictedImportsRule = [
  'error',
  {
    paths: restrictedFsdRootImports,
    patterns: restrictedFsdDeepImportPatterns,
  },
];

const disabledFsdRestrictedImportsRules = {
  'no-restricted-imports': 'off',
  '@typescript-eslint/no-restricted-imports': 'off',
};

function isFsdAliasReExportSource(value) {
  return (
    typeof value === 'string' &&
    FSD_ALIAS_PREFIXES.some((prefix) => value === prefix || value.startsWith(`${prefix}/`))
  );
}

const localArchitecturePlugin = {
  rules: {
    'no-fsd-alias-re-export': {
      meta: {
        type: 'problem',
        docs: {
          description:
            'Disallow transit alias re-exports from FSD layers. Public APIs must re-export their own local files through relative paths.',
        },
        schema: [],
        messages: {
          noTransitReExport:
            'Do not re-export from "{{ source }}" through an FSD alias. Public API files must re-export only local implementation through relative paths.',
        },
      },

      create(context) {
        function checkReExport(node) {
          const source = node.source?.value;

          if (!isFsdAliasReExportSource(source)) {
            return;
          }

          context.report({
            node: node.source,
            messageId: 'noTransitReExport',
            data: {
              source,
            },
          });
        }

        return {
          ExportAllDeclaration: checkReExport,
          ExportNamedDeclaration: checkReExport,
        };
      },
    },

    'intl-boundaries': intlBoundariesRule,
  },
};

/**
 * ESLint flat config:
 * - Oxlint owns native and type-aware rules for ordinary application and tooling TypeScript;
 * - strict type-aware ESLint rules stay enabled only for Svelte sources;
 * - JavaScript tooling plus architecture, Tailwind, and import rules stay in ESLint;
 * - Svelte recommended + formatter compatibility;
 * - Tailwind CSS v4 linting through better-tailwindcss recommended preset;
 * - cn/clsx/cx/cva Tailwind class detection through better-tailwindcss selectors;
 * - strict FSD topology through eslint-plugin-boundaries;
 * - public API import enforcement;
 * - public API files are treated as thin local facades;
 * - designated foundation entry points are excluded from FSD import restrictions;
 * - formatter compatibility is applied through eslint-config-prettier as the final override.
 */
export default defineConfig([
  globalIgnores(GLOBAL_IGNORES),

  {
    name: 'project/linter-options',

    linterOptions: {
      reportUnusedDisableDirectives: 'error',
      reportUnusedInlineConfigs: 'error',
    },
  },

  {
    name: 'project/ecmascript-options',

    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
    },
  },

  {
    name: 'project/browser-source-globals',

    files: SOURCE_FILE_GLOBS,

    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },

  {
    name: 'project/node-tooling-globals',

    files: TOOLING_JAVASCRIPT_FILE_GLOBS,

    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },

  ...eslintBaseConfigs,
  ...typescriptParserConfigs,
  ...typeCheckedTypeScriptConfigs,

  {
    name: 'project/type-aware-parser-options',

    files: SVELTE_TYPE_AWARE_FILE_GLOBS,

    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: PROJECT_ROOT,
      },
    },
  },

  ...svelte.configs['flat/recommended'],
  ...svelte.configs['flat/prettier'],

  {
    name: 'project/svelte-parser-options',

    files: SVELTE_FILE_GLOBS,

    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        projectService: true,
        tsconfigRootDir: PROJECT_ROOT,
        extraFileExtensions: ['.svelte'],
        svelteConfig,
      },
    },

    rules: {
      /*
       * Core ESLint can produce false positives with Svelte 5 runes and values
       * that are read only from markup. Svelte-specific rules still cover
       * actual template correctness.
       */
      'no-useless-assignment': 'off',

      /*
       * Base prefer-const is not Svelte-runes-aware and conflicts with
       * Svelte reactive declarations. svelte/prefer-const keeps const hygiene
       * for regular local variables while ignoring Svelte reactive values such
       * as $props and $derived by default.
       */
      'prefer-const': 'off',
      'svelte/prefer-const': [
        'error',
        {
          destructuring: 'all',
        },
      ],
    },
  },

  {
    name: 'project/javascript-rules',

    files: [...JAVASCRIPT_FILE_GLOBS, ...TOOLING_JAVASCRIPT_FILE_GLOBS],

    rules: {
      'no-unused-vars': ['error', UNUSED_VALUE_OPTIONS],
    },
  },

  {
    name: 'project/javascript-import-boundaries',

    files: JAVASCRIPT_FILE_GLOBS,

    rules: {
      'no-restricted-imports': fsdRestrictedImportsRule,
    },
  },

  {
    name: 'project/node-tooling-rules',

    files: TOOLING_JAVASCRIPT_FILE_GLOBS,

    rules: {
      curly: ['error', 'all'],
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      'no-var': 'error',
      'object-shorthand': ['error', 'always'],
      'prefer-const': ['error', { destructuring: 'all' }],
      'prefer-object-spread': 'error',
      'prefer-rest-params': 'error',
      'prefer-spread': 'error',
    },
  },

  {
    name: 'project/typescript-import-boundaries',

    files: [...TYPESCRIPT_FILE_GLOBS, ...SVELTE_COMPONENT_FILE_GLOBS],

    rules: {
      /*
       * This project-specific restriction is intentionally kept in ESLint
       * alongside the other FSD checks. It does not require TypeScript's type
       * service, and keeping one source of truth avoids duplicating its large
       * path policy in the Oxlint config.
       */
      'no-restricted-imports': 'off',
      '@typescript-eslint/no-restricted-imports': fsdRestrictedImportsRule,
    },
  },

  {
    name: 'project/svelte-type-aware-typescript-rules',

    files: SVELTE_TYPE_AWARE_FILE_GLOBS,

    rules: {
      'no-unused-vars': 'off',

      '@typescript-eslint/consistent-type-definitions': ['error', 'type'],
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          prefer: 'type-imports',
          fixStyle: 'inline-type-imports',
        },
      ],
      '@typescript-eslint/no-unused-vars': ['error', UNUSED_VALUE_OPTIONS],
      '@typescript-eslint/restrict-template-expressions': [
        'error',
        {
          allowNumber: true,
          allowBoolean: true,
        },
      ],
    },
  },

  {
    name: 'project/portable-source-rules',

    files: ESLINT_BASE_FILE_GLOBS,

    rules: {
      curly: ['error', 'all'],
      eqeqeq: ['error', 'always', { null: 'ignore' }],

      'no-console': ['warn', { allow: ['warn', 'error'] }],
    },
  },

  {
    name: 'project/duplicate-imports',

    files: SOURCE_FILE_GLOBS,

    rules: {
      /*
       * Public APIs use mixed type/value re-export lists. ESLint implements
       * the required allowSeparateTypeImports behavior correctly, and this
       * rule remains cheap because it does not need type information.
       */
      'no-duplicate-imports': [
        'error',
        {
          includeExports: true,
          allowSeparateTypeImports: true,
        },
      ],
    },
  },

  {
    name: 'project/architecture-rules',

    files: SOURCE_FILE_GLOBS,

    plugins: {
      'local-architecture': localArchitecturePlugin,
    },

    rules: {
      /*
       * Prevent public APIs from becoming transit barrels:
       * export from './model/foo' is OK;
       * export from '@/entities/user' is not OK.
       */
      'local-architecture/no-fsd-alias-re-export': 'error',
      'local-architecture/intl-boundaries': 'error',
    },
  },

  {
    name: 'project/foundation-entry-points',

    files: fsdBoundariesConfig.entryPointGlobs,

    rules: {
      /*
       * main/App/env declaration files are composition/bootstrap points rather
       * than regular FSD modules. They may import app wiring and global CSS.
       */
      ...disabledFsdRestrictedImportsRules,
    },
  },

  {
    name: 'project/tailwindcss-v4',

    files: SOURCE_FILE_GLOBS,

    extends: [betterTailwindcss.configs.recommended],

    rules: {
      /*
       * Oxfmt already handles line-wrapping at printWidth 100.
       * The Tailwind plugin's wrapping rule conflicts with Oxfmt's
       * formatting, causing endless fix loops.
       */
      'better-tailwindcss/enforce-consistent-line-wrapping': 'off',

      /*
       * Keep exactly one tool responsible for class ordering.
       *
       * Oxfmt owns class ordering through sortTailwindcss.
       * Use "error" here if class ordering is owned by ESLint instead.
       */
      'better-tailwindcss/enforce-consistent-class-order': 'off',

      /*
       * Canonicalization is intentionally disabled: it is stylistic and nearly
       * doubles cold lint time. Correctness-oriented Tailwind rules stay on,
       * while Oxfmt remains the single owner of class ordering.
       */
      'better-tailwindcss/enforce-canonical-classes': 'off',

      /*
       * shadcn-svelte Sonner uses a non-Tailwind root hook class upstream.
       */
      'better-tailwindcss/no-unknown-classes': ['error', { ignore: ['^toaster$'] }],
    },

    settings: {
      'better-tailwindcss': {
        /*
         * Tailwind CSS v4 uses the CSS entry point as the source of truth.
         */
        entryPoint: TAILWIND_ENTRY_POINT,

        /*
         * Allows better-tailwindcss to resolve TypeScript path aliases.
         */
        tsconfig: TYPESCRIPT_CONFIG,

        /*
         * Keeps Tailwind/config resolution stable when ESLint is run from the
         * repository root or through workspace scripts.
         */
        cwd: PROJECT_ROOT,

        /*
         * Preserve default selectors and add project-specific class composition
         * helpers. Without this, class strings inside cn()/cva() can be skipped.
         */
        selectors: tailwindClassSelectors,
      },
    },
  },

  {
    name: 'project/strict-fsd-boundaries',

    files: SOURCE_FILE_GLOBS,
    ignores: fsdBoundariesConfig.entryPointGlobs,

    plugins: {
      boundaries,
    },

    settings: fsdBoundariesConfig.settings,

    rules: fsdBoundariesConfig.rules,
  },

  {
    name: 'project/test-rules',

    files: TEST_FILE_GLOBS,

    languageOptions: {
      globals: {
        ...globals.vitest,
      },
    },

    rules: {
      /*
       * Tests stay architecture-strict by default because base JS/TS import
       * restrictions and boundaries rules still apply to test files.
       */
    },
  },

  eslintConfigPrettier,

  {
    name: 'project/formatter-safe-overrides',

    files: [...ESLINT_BASE_FILE_GLOBS, ...TOOLING_JAVASCRIPT_FILE_GLOBS],

    rules: {
      /*
       * eslint-config-prettier disables `curly` as a "special" rule, so it must
       * be re-asserted after the compatibility config or it stays off. The
       * `'all'` mode is formatter-safe: it only requires block braces, which
       * Oxfmt never adds or removes, so there is no formatting conflict.
       */
      curly: ['error', 'all'],
    },
  },
]);
