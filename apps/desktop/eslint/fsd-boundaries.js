import { strict as boundariesStrict } from 'eslint-plugin-boundaries/config';

const FSD_PUBLIC_API_CATEGORY = 'public-api';
const FSD_INTERNAL_CATEGORY = 'internal';

export const FSD_SLICED_LAYERS = Object.freeze(['pages', 'widgets', 'features', 'entities']);

export const FSD_ALIAS_PREFIXES = Object.freeze([
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

function requireNonEmptyString(value, name) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new TypeError(`${name} must be a non-empty string.`);
  }

  return value;
}

function requireExtensions(value, name) {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some((extension) => typeof extension !== 'string' || extension.length === 0)
  ) {
    throw new TypeError(`${name} must be a non-empty array of extension strings.`);
  }

  return value;
}

function extensionGlob(extensions) {
  return `{${extensions.join(',')}}`;
}

function publicApiFiles(sourceRoot, publicApiExtensions) {
  const extensions = extensionGlob(publicApiExtensions);

  return [
    ...FSD_SLICED_LAYERS.map((layer) => ({
      category: FSD_PUBLIC_API_CATEGORY,
      pattern: `${sourceRoot}/${layer}/*/index.${extensions}`,
    })),
    {
      category: FSD_PUBLIC_API_CATEGORY,
      pattern: `${sourceRoot}/shared/*/index.${extensions}`,
    },
  ];
}

function internalPatternsForElementRoot(elementRoot, publicApiExtensions, targetExtensions) {
  const publicFileNames = publicApiExtensions.map((extension) => `index.${extension}`);
  const nestedFileExtensions = extensionGlob(targetExtensions);

  return [
    `${elementRoot}/!(${publicFileNames.join('|')})@(*.${nestedFileExtensions})`,
    `${elementRoot}/*/**/*.${nestedFileExtensions}`,
  ];
}

function internalFiles(sourceRoot, publicApiExtensions, targetExtensions) {
  const extensions = extensionGlob(targetExtensions);

  return [
    {
      category: FSD_INTERNAL_CATEGORY,
      pattern: `${sourceRoot}/app/**/*.${extensions}`,
    },
    ...FSD_SLICED_LAYERS.map((layer) => ({
      category: FSD_INTERNAL_CATEGORY,
      pattern: internalPatternsForElementRoot(
        `${sourceRoot}/${layer}/*`,
        publicApiExtensions,
        targetExtensions,
      ),
    })),
    {
      category: FSD_INTERNAL_CATEGORY,
      pattern: internalPatternsForElementRoot(
        `${sourceRoot}/shared/*`,
        publicApiExtensions,
        targetExtensions,
      ),
    },
  ];
}

function entity(type, category, captured) {
  const element = {
    types: [type],
  };

  if (captured !== undefined) {
    element.captured = captured;
  }

  return {
    element,
    file: {
      categories: [category],
    },
  };
}

function publicApiOf(type) {
  return entity(type, FSD_PUBLIC_API_CATEGORY);
}

function internalOf(type, captured) {
  return entity(type, FSD_INTERNAL_CATEGORY, captured);
}

function sameInternalSliceOf(type) {
  return internalOf(type, {
    slice: '{{ from.element.captured.slice }}',
  });
}

function sameSharedSegmentInternal() {
  return internalOf('shared', {
    segment: '{{ from.element.captured.segment }}',
  });
}

function createDependencyPolicy(from, targets) {
  return {
    from,
    allow: targets.map((target) => ({ to: target })),
  };
}

function publicApisAvailableForLayer() {
  return {
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
}

function slicedLayerPolicies(layer, publicApisByLayer) {
  return [
    createDependencyPolicy(publicApiOf(layer), [sameInternalSliceOf(layer)]),
    createDependencyPolicy(internalOf(layer), [
      sameInternalSliceOf(layer),
      ...publicApisByLayer[layer],
    ]),
  ];
}

export function createFsdBoundariesConfig({
  rootPath,
  sourceRoot,
  publicApiExtensions,
  targetExtensions,
  resolverExtensions,
  typescriptConfigPath,
}) {
  requireNonEmptyString(rootPath, 'rootPath');
  requireNonEmptyString(sourceRoot, 'sourceRoot');
  requireNonEmptyString(typescriptConfigPath, 'typescriptConfigPath');
  requireExtensions(publicApiExtensions, 'publicApiExtensions');
  requireExtensions(targetExtensions, 'targetExtensions');
  requireExtensions(resolverExtensions, 'resolverExtensions');

  const entryPointGlobs = [
    `${sourceRoot}/main.{js,ts}`,
    `${sourceRoot}/App.svelte`,
    `${sourceRoot}/app.d.ts`,
    `${sourceRoot}/vite-env.d.ts`,
  ];
  const elements = [
    {
      type: 'app',
      pattern: `${sourceRoot}/app`,
      partialMatch: false,
    },
    ...FSD_SLICED_LAYERS.map((layer) => ({
      type: layer,
      pattern: `${sourceRoot}/${layer}/*`,
      capture: ['slice'],
      partialMatch: false,
    })),
    {
      type: 'shared',
      pattern: `${sourceRoot}/shared/*`,
      capture: ['segment'],
      partialMatch: false,
    },
  ];
  const publicApisByLayer = publicApisAvailableForLayer();
  const policies = [
    createDependencyPolicy(internalOf('app'), [
      internalOf('app'),
      ...FSD_SLICED_LAYERS.map(publicApiOf),
      publicApiOf('shared'),
    ]),
    ...FSD_SLICED_LAYERS.flatMap((layer) => slicedLayerPolicies(layer, publicApisByLayer)),
    createDependencyPolicy(publicApiOf('shared'), [sameSharedSegmentInternal()]),
    createDependencyPolicy(internalOf('shared'), [
      sameSharedSegmentInternal(),
      publicApiOf('shared'),
    ]),
  ];

  return {
    entryPointGlobs,
    settings: {
      ...boundariesStrict.settings,
      'boundaries/root-path': rootPath,
      'boundaries/ignore': entryPointGlobs,
      'boundaries/legacy-templates': false,
      'boundaries/legacy-warnings': false,
      'boundaries/elements-single-match': true,
      'boundaries/files-single-match': true,
      'import/resolver': {
        typescript: {
          alwaysTryTypes: true,
          project: [typescriptConfigPath],
        },
        node: {
          extensions: [...resolverExtensions],
        },
      },
      'boundaries/elements': elements,
      'boundaries/files': [
        ...publicApiFiles(sourceRoot, publicApiExtensions),
        ...internalFiles(sourceRoot, publicApiExtensions, targetExtensions),
      ],
    },
    rules: {
      'boundaries/no-unknown-files': 'error',
      'boundaries/no-unknown-dependencies': ['error', { require: 'all' }],
      'boundaries/no-ignored-dependencies': 'error',
      'boundaries/dependencies': [
        'error',
        {
          default: 'disallow',
          checkUnknownLocals: true,
          checkInternals: true,
          message:
            'FSD violation: "{{ from.element.types.[0] }}" cannot import "{{ to.element.types.[0] }}". Use public APIs between slices/layers and relative imports inside the same slice or shared segment.',
          policies,
        },
      ],
    },
  };
}
