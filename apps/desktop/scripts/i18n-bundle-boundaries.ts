import { LAZY_LOCALES } from '../ui/src/shared/i18n/locale-model.ts';
import {
  isI18nOverrideModule,
  localeModuleOwner,
  MESSAGE_ROOT,
  PACK_ROOT,
} from './i18n-bundle-boundaries/locale-ownership.ts';

export type BundleChunk = {
  type: 'chunk';
  fileName: string;
  facadeModuleId: string | null;
  imports: string[];
  dynamicImports: string[];
  isEntry: boolean;
  isDynamicEntry: boolean;
  modules: Record<string, unknown>;
};

type BundleAsset = BundleChunk | { type: 'asset'; fileName: string; source?: unknown };
export type OutputBundleLike = Record<string, BundleAsset>;

function normalize(value: string): string {
  return value.replaceAll('\\', '/');
}

function hasSourceSuffix(value: string, suffix: string): boolean {
  return normalize(value).endsWith(suffix);
}

function modulePreloadReferences(html: string): ReadonlySet<string> {
  const references = new Set<string>();
  const linkTags = html.matchAll(/<link\b[^>]*>/giu);

  for (const match of linkTags) {
    const attributes = new Map<string, string>();
    const attributePattern = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/gu;
    const attributeSource = match[0].slice('<link'.length, -1);

    for (const attribute of attributeSource.matchAll(attributePattern)) {
      const name = attribute[1].toLowerCase();
      const value = attribute[2] ?? attribute[3] ?? attribute[4];
      if (value !== undefined) {
        attributes.set(name, value);
      }
    }

    const rel = attributes.get('rel');
    const href = attributes.get('href');
    if (rel?.toLowerCase().split(/\s+/u).includes('modulepreload') && href !== undefined) {
      const path = normalize(new URL(href, 'https://renderpilot.invalid/').pathname).replace(
        /^\/+|\/+$/gu,
        '',
      );
      if (path !== '') {
        references.add(path);
      }
    }
  }

  return references;
}

function isModulePreloaded(references: ReadonlySet<string>, fileName: string): boolean {
  const normalizedFileName = normalize(fileName).replace(/^\/+|\/+$/gu, '');
  for (const reference of references) {
    if (reference === normalizedFileName || reference.endsWith(`/${normalizedFileName}`)) {
      return true;
    }
  }
  return false;
}

function textAssetSource(source: unknown): string | null {
  if (typeof source === 'string') {
    return source;
  }
  if (source instanceof Uint8Array) {
    return new TextDecoder().decode(source);
  }
  return null;
}

function staticReachable(bundle: OutputBundleLike, rootFileName: string): Set<string> {
  const reachable = new Set<string>();
  const visit = (fileName: string): void => {
    if (reachable.has(fileName)) {
      return;
    }

    const chunk = bundle[fileName];
    if (!chunk || chunk.type !== 'chunk') {
      return;
    }

    reachable.add(fileName);
    for (const dependency of chunk.imports) {
      visit(dependency);
    }
  };

  visit(rootFileName);
  return reachable;
}

function dynamicReachable(bundle: OutputBundleLike, roots: Set<string>): Set<string> {
  const reachable = new Set<string>();
  const visit = (fileName: string): void => {
    if (reachable.has(fileName)) {
      return;
    }

    const chunk = bundle[fileName];
    if (!chunk || chunk.type !== 'chunk') {
      return;
    }

    reachable.add(fileName);
    for (const dependency of chunk.imports) {
      visit(dependency);
    }
    for (const dependency of chunk.dynamicImports) {
      visit(dependency);
    }
  };

  for (const root of roots) {
    const chunk = bundle[root];
    if (!chunk || chunk.type !== 'chunk') {
      continue;
    }

    for (const dependency of chunk.dynamicImports) {
      visit(dependency);
    }
  }

  return reachable;
}

function moduleIds(bundle: OutputBundleLike, chunkNames: Set<string>): string[] {
  return chunkNames
    .values()
    .flatMap((fileName) => {
      const chunk = bundle[fileName];
      return chunk?.type === 'chunk' ? Object.keys(chunk.modules) : [];
    })
    .toArray();
}

function findChunkByFacade(bundle: OutputBundleLike, suffix: string): BundleChunk {
  const matches = Object.values(bundle).filter(
    (asset): asset is BundleChunk =>
      asset.type === 'chunk' &&
      asset.facadeModuleId !== null &&
      hasSourceSuffix(asset.facadeModuleId, suffix),
  );

  if (matches.length !== 1) {
    throw new Error(`Expected one chunk for ${suffix}, found ${matches.length}.`);
  }

  return matches[0];
}

function findChunkContainingModule(bundle: OutputBundleLike, suffix: string): BundleChunk {
  const matches = Object.values(bundle).filter(
    (asset): asset is BundleChunk =>
      asset.type === 'chunk' &&
      Object.keys(asset.modules).some((moduleId) => hasSourceSuffix(moduleId, suffix)),
  );

  if (matches.length !== 1) {
    throw new Error(`Expected one chunk containing ${suffix}, found ${matches.length}.`);
  }

  return matches[0];
}

export function assertI18nBundleBoundaries(bundle: OutputBundleLike): void {
  const chunks = Object.values(bundle).filter((asset) => asset.type === 'chunk');
  const entries = chunks.filter((chunk) => chunk.isEntry);

  if (entries.length !== 1) {
    throw new Error(`Expected one application entry, found ${entries.length}.`);
  }

  const bootstrap = entries[0];
  const desktopApp = findChunkByFacade(bundle, '/ui/src/app/routes/DesktopApp.svelte');
  const indexHtml = Object.values(bundle).find(
    (asset): asset is Extract<BundleAsset, { type: 'asset' }> =>
      asset.type === 'asset' && asset.fileName === 'index.html',
  );
  if (indexHtml === undefined) {
    throw new Error('Expected index.html in the generated bundle.');
  }
  const indexHtmlSource = textAssetSource(indexHtml.source);
  if (indexHtmlSource === null) {
    throw new Error('Expected index.html to contain textual HTML source.');
  }
  const modulePreloads = modulePreloadReferences(indexHtmlSource);
  const initialChunks = staticReachable(bundle, bootstrap.fileName).union(
    staticReachable(bundle, desktopApp.fileName),
  );
  const initialModules = moduleIds(bundle, initialChunks);

  const englishPack = `${PACK_ROOT}en.ts`;
  if (!initialModules.some((moduleId) => hasSourceSuffix(moduleId, englishPack))) {
    throw new Error('The eager English locale pack is missing from the initial module graph.');
  }
  if (!initialModules.some((moduleId) => hasSourceSuffix(moduleId, `${MESSAGE_ROOT}en.ts`))) {
    throw new Error('The eager English message catalog is missing from the initial module graph.');
  }

  const leakedModules = initialChunks
    .values()
    .flatMap((fileName) => {
      const chunk = bundle[fileName];
      if (!chunk || chunk.type !== 'chunk') {
        return [];
      }

      return Object.keys(chunk.modules)
        .filter(
          (moduleId) => localeModuleOwner(moduleId) !== null || isI18nOverrideModule(moduleId),
        )
        .map((moduleId) => ({ fileName, moduleId }));
    })
    .toArray();

  if (leakedModules.length > 0) {
    throw new Error(
      `Non-active locale modules leaked into the initial graph:\n${leakedModules
        .map(({ fileName, moduleId }) => `- ${moduleId} (initial chunk: ${fileName})`)
        .join('\n')}`,
    );
  }

  const dynamicChunks = dynamicReachable(bundle, initialChunks);
  const registryChunk = findChunkContainingModule(bundle, `${PACK_ROOT}registry.ts`);
  for (const locale of LAZY_LOCALES) {
    const pack = findChunkByFacade(bundle, `${PACK_ROOT}${locale}.ts`);

    if (!pack.isDynamicEntry) {
      throw new Error(`Locale pack ${locale} is not emitted as a dynamic entry.`);
    }
    if (initialChunks.has(pack.fileName)) {
      throw new Error(`Locale pack ${locale} is present in the initial chunk graph.`);
    }
    if (!dynamicChunks.has(pack.fileName)) {
      throw new Error(`Locale pack ${locale} is not dynamically reachable from bootstrap.`);
    }
    if (!registryChunk.dynamicImports.includes(pack.fileName)) {
      throw new Error(
        `Locale pack ${locale} is not a direct dynamic import of the locale loader registry.`,
      );
    }

    const localeGraph = staticReachable(bundle, pack.fileName).difference(initialChunks);
    const crossLocaleModules = moduleIds(bundle, localeGraph).filter((moduleId) => {
      const owner = localeModuleOwner(moduleId);
      return owner !== null && owner !== locale;
    });
    if (crossLocaleModules.length > 0) {
      throw new Error(
        `Locale graph ${locale} imports modules owned by another locale:\n${crossLocaleModules
          .map((moduleId) => `- ${moduleId}`)
          .join('\n')}`,
      );
    }

    // Locale-neutral chunks may already belong to the verified initial graph.
    // Any other dependency of a locale pack must stay out of HTML preloads.
    for (const graphFileName of staticReachable(bundle, pack.fileName)) {
      const graphChunk = bundle[graphFileName];
      if (
        !initialChunks.has(graphFileName) &&
        graphChunk?.type === 'chunk' &&
        isModulePreloaded(modulePreloads, graphChunk.fileName)
      ) {
        throw new Error(`Locale graph ${locale} was preloaded by index.html: ${graphFileName}`);
      }
    }
  }
}
