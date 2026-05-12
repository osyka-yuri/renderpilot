import path from 'node:path';
import { fileURLToPath } from 'node:url';

import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import prettier from 'eslint-config-prettier';
import betterTailwindcss from 'eslint-plugin-better-tailwindcss';
import { getDefaultSelectors } from 'eslint-plugin-better-tailwindcss/defaults';
import boundaries from 'eslint-plugin-boundaries';
import { strict as boundariesStrict } from 'eslint-plugin-boundaries/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import tseslint from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

const PROJECT_ROOT = path.dirname(fileURLToPath(import.meta.url));

const SOURCE_ROOT = 'ui/src';
const TAILWIND_ENTRY_POINT = `${SOURCE_ROOT}/shared/theme/global.css`;
const TYPESCRIPT_CONFIG = './tsconfig.json';

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

const FSD_PUBLIC_API_CATEGORY = 'public-api';
const FSD_INTERNAL_CATEGORY = 'internal';

const FSD_SLICED_LAYERS = ['pages', 'widgets', 'features', 'entities'];

const FSD_ALIAS_PREFIXES = [
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
  'prettier.config.{js,cjs,mjs,ts}',
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

const LINTED_SOURCE_FILE_GLOBS = sourceFiles(SOURCE_SCRIPT_EXTENSIONS);
const JAVASCRIPT_SOURCE_FILE_GLOBS = sourceFiles(JAVASCRIPT_EXTENSIONS);
const TYPESCRIPT_SOURCE_FILE_GLOBS = sourceFiles(TYPESCRIPT_EXTENSIONS);

const SVELTE_FILE_GLOBS = [`${SOURCE_ROOT}/**/*.svelte`, `${SOURCE_ROOT}/**/*.svelte.{js,ts}`];

const TYPE_CHECKED_FILE_GLOBS = [...TYPESCRIPT_SOURCE_FILE_GLOBS, `${SOURCE_ROOT}/**/*.svelte`];

const TEST_FILE_GLOBS = [
  `${SOURCE_ROOT}/**/*.{test,spec}.{${toBraceGlob(SOURCE_SCRIPT_EXTENSIONS)}}`,
];

const ARCHITECTURE_IMPORT_TARGET_GLOBS = [
  `${SOURCE_ROOT}/**/*.{${toBraceGlob([...SOURCE_SCRIPT_EXTENSIONS, 'css'])}}`,
];

const FSD_ENTRY_POINT_GLOBS = [
  `${SOURCE_ROOT}/main.{js,ts}`,
  `${SOURCE_ROOT}/App.svelte`,
  `${SOURCE_ROOT}/app.d.ts`,
  `${SOURCE_ROOT}/vite-env.d.ts`,
];

const FSD_PUBLIC_API_FILE_NAMES = [
  ...SOURCE_SCRIPT_EXTENSIONS.map((extension) => `index.${extension}`),
  ...SVELTE_MODULE_EXTENSIONS.map((extension) => `index.${extension}`),
];

const javascriptRecommendedConfigs = scopeConfigs(
  [js.configs.recommended],
  LINTED_SOURCE_FILE_GLOBS,
);

const typeCheckedTypeScriptConfigs = scopeConfigs(
  [...tseslint.configs.strictTypeChecked, ...tseslint.configs.stylisticTypeChecked],
  TYPE_CHECKED_FILE_GLOBS,
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

function createPublicApiPatterns(layerRoot) {
  return FSD_PUBLIC_API_FILE_NAMES.map((fileName) => `${layerRoot}/*/${fileName}`);
}

function createSlicedLayerElements(layer) {
  const layerRoot = `${SOURCE_ROOT}/${layer}`;

  return [
    {
      type: layer,
      category: FSD_PUBLIC_API_CATEGORY,
      pattern: createPublicApiPatterns(layerRoot),
      mode: 'full',
      capture: ['slice'],
    },
    {
      type: layer,
      category: FSD_INTERNAL_CATEGORY,
      pattern: `${layerRoot}/*`,
      mode: 'folder',
      capture: ['slice'],
    },
  ];
}

const fsdElements = [
  {
    type: 'app',
    category: FSD_INTERNAL_CATEGORY,
    pattern: `${SOURCE_ROOT}/app`,
    mode: 'folder',
  },

  ...FSD_SLICED_LAYERS.flatMap(createSlicedLayerElements),

  {
    type: 'shared',
    category: FSD_PUBLIC_API_CATEGORY,
    pattern: createPublicApiPatterns(`${SOURCE_ROOT}/shared`),
    mode: 'full',
    capture: ['segment'],
  },
  {
    type: 'shared',
    category: FSD_INTERNAL_CATEGORY,
    pattern: `${SOURCE_ROOT}/shared/*`,
    mode: 'folder',
    capture: ['segment'],
  },
];

function publicApiOf(type) {
  return {
    type,
    category: FSD_PUBLIC_API_CATEGORY,
  };
}

function internalOf(type) {
  return {
    type,
    category: FSD_INTERNAL_CATEGORY,
  };
}

function sameInternalSliceOf(type) {
  return {
    type,
    category: FSD_INTERNAL_CATEGORY,
    captured: {
      slice: '{{ from.captured.slice }}',
    },
  };
}

const sameSharedSegmentInternal = {
  type: 'shared',
  category: FSD_INTERNAL_CATEGORY,
  captured: {
    segment: '{{ from.captured.segment }}',
  },
};

function createDependencyRule(from, to) {
  return {
    from,
    allow: {
      to,
    },
  };
}

const publicApisAvailableForLayer = {
  pages: [
    publicApiOf('widgets'),
    publicApiOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ],
  widgets: [publicApiOf('features'), publicApiOf('entities'), publicApiOf('shared')],
  features: [publicApiOf('entities'), publicApiOf('shared')],
  entities: [publicApiOf('shared')],
};

function createSlicedLayerDependencyRules(layer) {
  return [
    /*
     * Public API must be a thin local facade.
     * It should re-export local implementation, not compose lower layers.
     */
    createDependencyRule(publicApiOf(layer), [sameInternalSliceOf(layer)]),

    /*
     * Internal files may use their own slice internals and public APIs of lower layers.
     * Importing the same slice through its own public API is intentionally disallowed:
     * use relative imports inside the slice to avoid circular barrels.
     */
    createDependencyRule(internalOf(layer), [
      sameInternalSliceOf(layer),
      ...publicApisAvailableForLayer[layer],
    ]),
  ];
}

const fsdDependencyRules = [
  createDependencyRule(internalOf('app'), [
    internalOf('app'),
    publicApiOf('pages'),
    publicApiOf('widgets'),
    publicApiOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ]),

  ...FSD_SLICED_LAYERS.flatMap(createSlicedLayerDependencyRules),

  /*
   * Shared segment public APIs should also stay thin local facades.
   */
  createDependencyRule(publicApiOf('shared'), [sameSharedSegmentInternal]),

  /*
   * Shared internals may use their own segment internals and other shared segment
   * public APIs. They may not import entities/features/widgets/pages/app.
   */
  createDependencyRule(internalOf('shared'), [sameSharedSegmentInternal, publicApiOf('shared')]),
];

const fsdBoundariesRules = {
  'boundaries/no-unknown-files': 'error',
  'boundaries/no-unknown': 'error',
  'boundaries/no-ignored': 'error',

  'boundaries/dependencies': [
    'error',
    {
      default: 'disallow',
      checkUnknownLocals: true,
      checkInternals: true,
      message:
        'FSD violation: "{{ from.type }}" cannot import "{{ to.type }}". Use public APIs between slices/layers and relative imports inside the same slice or shared segment.',
      rules: fsdDependencyRules,
    },
  ],
};

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
  },
};

const fsdBoundariesSettings = {
  ...boundariesStrict.settings,

  'boundaries/root-path': PROJECT_ROOT,
  'boundaries/include': ARCHITECTURE_IMPORT_TARGET_GLOBS,
  'boundaries/ignore': FSD_ENTRY_POINT_GLOBS,

  /*
   * Prefer modern Handlebars templates:
   * "{{ from.captured.slice }}" instead of legacy captured shortcuts.
   */
  'boundaries/legacy-templates': false,

  'import/resolver': {
    typescript: {
      alwaysTryTypes: true,
      project: [TYPESCRIPT_CONFIG],
    },
    node: {
      extensions: RESOLVER_EXTENSIONS,
    },
  },

  'boundaries/elements': fsdElements,
};

/**
 * ESLint flat config:
 * - strict type-aware TypeScript only for TS/Svelte sources;
 * - JS sources stay non-type-aware;
 * - Svelte recommended + Svelte Prettier compatibility;
 * - Tailwind CSS v4 linting through better-tailwindcss recommended preset;
 * - cn/clsx/cx/cva Tailwind class detection through better-tailwindcss selectors;
 * - strict FSD topology through eslint-plugin-boundaries;
 * - public API import enforcement;
 * - public API files are treated as thin local facades;
 * - bootstrap files are intentionally excluded from FSD import restrictions;
 * - Prettier compatibility is applied as the final override.
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

    files: LINTED_SOURCE_FILE_GLOBS,

    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },

  ...javascriptRecommendedConfigs,
  ...typeCheckedTypeScriptConfigs,

  {
    name: 'project/type-aware-parser-options',

    files: TYPE_CHECKED_FILE_GLOBS,

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

    files: JAVASCRIPT_SOURCE_FILE_GLOBS,

    rules: {
      'no-unused-vars': ['error', UNUSED_VALUE_OPTIONS],
      'no-restricted-imports': fsdRestrictedImportsRule,
    },
  },

  {
    name: 'project/typescript-rules',

    files: TYPE_CHECKED_FILE_GLOBS,

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

      /*
       * Prefer the TypeScript-aware extension rule for TS/Svelte sources.
       * It handles type-only imports better than the core ESLint rule.
       */
      'no-restricted-imports': 'off',
      '@typescript-eslint/no-restricted-imports': fsdRestrictedImportsRule,
    },
  },

  {
    name: 'project/base-source-rules',

    files: LINTED_SOURCE_FILE_GLOBS,

    plugins: {
      'local-architecture': localArchitecturePlugin,
    },

    rules: {
      curly: ['error', 'all'],
      eqeqeq: ['error', 'always', { null: 'ignore' }],

      'no-console': ['warn', { allow: ['warn', 'error'] }],
      'no-duplicate-imports': [
        'error',
        {
          includeExports: true,
          allowSeparateTypeImports: true,
        },
      ],

      /*
       * Prevent public APIs from becoming transit barrels:
       * export from './model/foo' is OK;
       * export from '@/entities/user' is not OK.
       */
      'local-architecture/no-fsd-alias-re-export': 'error',
    },
  },

  {
    name: 'project/foundation-entry-points',

    files: FSD_ENTRY_POINT_GLOBS,

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

    files: LINTED_SOURCE_FILE_GLOBS,

    extends: [betterTailwindcss.configs.recommended],

    rules: {
      /*
       * Prettier already handles line-wrapping at printWidth 100.
       * The Tailwind plugin's wrapping rule conflicts with Prettier's
       * formatting, causing endless fix loops.
       */
      'better-tailwindcss/enforce-consistent-line-wrapping': 'off',

      /*
       * Keep exactly one tool responsible for class ordering.
       *
       * Use "off" if prettier-plugin-tailwindcss is enabled.
       * Use "error" here if class ordering is owned by ESLint instead.
       */
      'better-tailwindcss/enforce-consistent-class-order': 'off',
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

    files: LINTED_SOURCE_FILE_GLOBS,
    ignores: FSD_ENTRY_POINT_GLOBS,

    plugins: {
      boundaries,
    },

    settings: fsdBoundariesSettings,

    rules: fsdBoundariesRules,
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

  prettier,
]);
