import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import prettier from 'eslint-config-prettier';
import boundaries from 'eslint-plugin-boundaries';
import { strict as boundariesStrict } from 'eslint-plugin-boundaries/config';
import globals from 'globals';
import svelte from 'eslint-plugin-svelte';
import tseslint from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

const PROJECT_ROOT = dirname(fileURLToPath(import.meta.url));

const TYPESCRIPT_RESOLVER_PROJECTS = ['./tsconfig.json'];

const SCRIPT_FILE_EXTENSIONS = [
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.ts',
  '.tsx',
  '.mts',
  '.cts',
  '.svelte',
  '.svelte.js',
  '.svelte.ts',
];

const RESOLVABLE_FILE_EXTENSIONS = [...SCRIPT_FILE_EXTENSIONS, '.css'];

const LINTED_SOURCE_FILE_GLOBS = ['ui/src/**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts,svelte}'];

const ARCHITECTURE_IMPORT_TARGET_GLOBS = ['ui/src/**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts,svelte,css}'];

const SVELTE_FILE_GLOBS = ['ui/src/**/*.svelte', 'ui/src/**/*.svelte.{js,ts}'];
const TEST_FILE_GLOBS = ['ui/src/**/*.test.*', 'ui/src/**/*.spec.*'];

const GLOBAL_IGNORES = [
  '**/dist/**',
  '**/build/**',
  '**/.svelte-kit/**',
  '**/node_modules/**',
  '**/coverage/**',
  '**/pnpm-lock.yaml',
  '**/src-tauri/target/**',
  '**/src-tauri/gen/**',

  /*
   * CSS is not parsed as an ESLint source file here.
   * It is still included in ARCHITECTURE_IMPORT_TARGET_GLOBS so imports
   * such as "./styles.css" can be classified as architecture targets.
   */
  '**/*.css',

  'eslint.config.js',
  'prettier.config.mjs',
  'svelte.config.js',
];

const BOUNDARIES_TOPOLOGY_IGNORES = [
  'ui/src/main.{js,ts}',
  'ui/src/App.svelte',
  'ui/src/app.d.ts',
  'ui/src/vite-env.d.ts',
];

const FSD_PUBLIC_API_CATEGORY = 'public-api';
const FSD_INTERNAL_CATEGORY = 'internal';

const FSD_SLICED_LAYERS = ['pages', 'widgets', 'features', 'entities'];

const FSD_PUBLIC_API_FILE_NAMES = [
  'index.js',
  'index.jsx',
  'index.ts',
  'index.tsx',
  'index.svelte',
];

function createPublicApiPatterns(layerRoot) {
  return FSD_PUBLIC_API_FILE_NAMES.map((fileName) => `${layerRoot}/*/${fileName}`);
}

function createSlicedLayerElements(layer) {
  return [
    {
      type: layer,
      category: FSD_PUBLIC_API_CATEGORY,
      pattern: createPublicApiPatterns(`ui/src/${layer}`),
      mode: 'full',
      capture: ['slice'],
    },
    {
      type: layer,
      category: FSD_INTERNAL_CATEGORY,
      pattern: `ui/src/${layer}/*`,
      mode: 'folder',
      capture: ['slice'],
    },
  ];
}

const fsdElements = [
  {
    type: 'app',
    category: FSD_INTERNAL_CATEGORY,
    pattern: 'ui/src/app',
    mode: 'folder',
  },

  ...FSD_SLICED_LAYERS.flatMap(createSlicedLayerElements),

  {
    type: 'shared',
    category: FSD_PUBLIC_API_CATEGORY,
    pattern: createPublicApiPatterns('ui/src/shared'),
    mode: 'full',
    capture: ['segment'],
  },
  {
    type: 'shared',
    category: FSD_INTERNAL_CATEGORY,
    pattern: 'ui/src/shared/*',
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

function sameSliceOf(type) {
  return {
    type,
    captured: {
      slice: '{{ from.captured.slice }}',
    },
  };
}

const sameSharedSegment = {
  type: 'shared',
  captured: {
    segment: '{{ from.captured.segment }}',
  },
};

function allowDependencies(from, to) {
  return {
    from: { type: from },
    allow: { to },
  };
}

const fsdDependencyRules = [
  allowDependencies('app', [
    { type: 'app' },
    publicApiOf('pages'),
    publicApiOf('widgets'),
    publicApiOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ]),

  allowDependencies('pages', [
    sameSliceOf('pages'),
    publicApiOf('widgets'),
    publicApiOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ]),

  allowDependencies('widgets', [
    sameSliceOf('widgets'),
    publicApiOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ]),

  allowDependencies('features', [
    sameSliceOf('features'),
    publicApiOf('entities'),
    publicApiOf('shared'),
  ]),

  allowDependencies('entities', [sameSliceOf('entities'), publicApiOf('shared')]),

  allowDependencies('shared', [sameSharedSegment, publicApiOf('shared')]),
];

const fsdBoundariesRules = {
  /*
   * Keep the production topology closed:
   * - every checked file must belong to a known FSD element;
   * - known files cannot import unknown files;
   * - known files cannot import explicitly ignored architecture targets.
   */
  'boundaries/no-unknown-files': 'error',
  'boundaries/no-unknown': 'error',
  'boundaries/no-ignored': 'error',

  'boundaries/dependencies': [
    'error',
    {
      default: 'disallow',
      checkUnknownLocals: true,
      checkInternals: true,
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
        `Import a concrete slice public API, for example "@/features/auth" or "@features/auth".`,
    },
    {
      name: `@${layer}`,
      message:
        `Do not import the "${layer}" FSD layer root directly. ` +
        `Import a concrete slice public API, for example "@/features/auth" or "@features/auth".`,
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
      'Do not import the shared layer root directly. Import a concrete shared segment public API, for example "@/shared/ui" or "@shared/ui".',
  },
  {
    name: '@shared',
    message:
      'Do not import the shared layer root directly. Import a concrete shared segment public API, for example "@/shared/ui" or "@shared/ui".',
  },
];

const restrictedFsdDeepImportPatterns = [
  {
    group: FSD_SLICED_LAYERS.flatMap(createSlicedLayerDeepImportPatterns),
    message:
      'Import FSD slices only through their public API: "@/features/slice" / "@features/slice". Internal slice files must use relative imports inside the same slice.',
  },
  {
    group: ['@/shared/*/*', '@/shared/*/*/**', '@shared/*/*', '@shared/*/*/**'],
    message:
      'Import shared code through a segment public API, for example "@/shared/ui" or "@shared/ui". Internal shared files must use relative imports inside the same shared segment.',
  },
];

/**
 * ESLint flat config:
 * - strict type-aware TypeScript;
 * - Svelte recommended rules;
 * - strict FSD topology through eslint-plugin-boundaries;
 * - public API import enforcement;
 * - Prettier compatibility.
 */
export default defineConfig([
  globalIgnores(GLOBAL_IGNORES),

  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,

  /*
   * Svelte recommended enables Svelte-specific correctness rules.
   * Svelte prettier disables rules that conflict with Prettier.
   */
  ...svelte.configs['flat/recommended'],
  ...svelte.configs['flat/prettier'],

  {
    name: 'project/language-options',

    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: {
        ...globals.browser,
        ...globals.node,
      },
      parserOptions: {
        projectService: true,
        tsconfigRootDir: PROJECT_ROOT,
      },
    },
  },

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
  },

  {
    name: 'project/strict-fsd-boundaries',

    files: LINTED_SOURCE_FILE_GLOBS,
    ignores: BOUNDARIES_TOPOLOGY_IGNORES,

    plugins: {
      boundaries,
    },

    settings: {
      ...boundariesStrict.settings,

      'boundaries/root-path': PROJECT_ROOT,
      'boundaries/include': ARCHITECTURE_IMPORT_TARGET_GLOBS,
      'boundaries/ignore': BOUNDARIES_TOPOLOGY_IGNORES,

      /*
       * Prefer modern Handlebars templates:
       * "{{ from.captured.slice }}" instead of legacy captured shortcuts.
       */
      'boundaries/legacy-templates': false,

      /*
       * Required for aliases such as "@/..." / "@features/...".
       */
      'import/resolver': {
        typescript: {
          alwaysTryTypes: true,
          project: TYPESCRIPT_RESOLVER_PROJECTS,
        },
        node: {
          extensions: RESOLVABLE_FILE_EXTENSIONS,
        },
      },

      'boundaries/elements': fsdElements,
    },

    rules: fsdBoundariesRules,
  },

  {
    name: 'project/base-rules',

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

      '@typescript-eslint/consistent-type-definitions': ['error', 'type'],
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          prefer: 'type-imports',
          fixStyle: 'inline-type-imports',
        },
      ],
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/restrict-template-expressions': [
        'error',
        {
          allowNumber: true,
          allowBoolean: true,
        },
      ],

      /*
       * Use the TypeScript-aware extension rule.
       * It handles type-only imports correctly.
       */
      'no-restricted-imports': 'off',
      '@typescript-eslint/no-restricted-imports': [
        'error',
        {
          paths: restrictedFsdRootImports,
          patterns: restrictedFsdDeepImportPatterns,
        },
      ],
    },
  },

  {
    name: 'project/test-fsd-public-api-imports',

    files: TEST_FILE_GLOBS,

    rules: {
      '@typescript-eslint/no-restricted-imports': [
        'error',
        {
          paths: restrictedFsdRootImports,
          patterns: restrictedFsdDeepImportPatterns,
        },
      ],
    },
  },

  prettier,
]);
