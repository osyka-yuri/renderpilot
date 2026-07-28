import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';

const root = process.cwd();
const dist = path.join(root, 'dist');
const manifest = JSON.parse(await readFile(path.join(dist, '.vite', 'manifest.json'), 'utf8'));

const INITIAL_JS_BUDGET_BYTES = 1_500_000;
const ROUTE_JS_BUDGET_BYTES = 500_000;
const LAZY_PAGE_ENTRIES = {
  details: '/pages/game-details/ui/GameDetailsPage.svelte',
  operations: '/pages/operations/ui/OperationsPage.svelte',
  settings: '/pages/settings/ui/SettingsPage.svelte',
  libraries: '/pages/libraries/ui/LibrariesPage.svelte',
};
const UPDATE_DIALOG_ENTRY = '/features/app-updater/ui/AppUpdateDialog.svelte';

const entries = Object.entries(manifest);
const normalizeKey = (key) => `/${key.replaceAll('\\', '/')}`;
const findEntryKey = (sourceSuffix) => {
  const matches = entries
    .map(([key]) => key)
    .filter((key) => normalizeKey(key).endsWith(sourceSuffix));

  if (matches.length !== 1) {
    throw new Error(
      `Expected one manifest entry ending with ${sourceSuffix}, found ${matches.length}.`,
    );
  }

  return matches[0];
};

const entryKeys = entries.filter(([, chunk]) => chunk.isEntry).map(([key]) => key);
if (entryKeys.length !== 1) {
  throw new Error(`Expected one application entry, found ${entryKeys.length}.`);
}

const staticGraph = (rootKey) => {
  const keys = new Set();
  const visit = (key) => {
    if (keys.has(key)) return;
    keys.add(key);
    for (const dependency of manifest[key]?.imports ?? []) visit(dependency);
  };
  visit(rootKey);
  return keys;
};
const desktopAppKey = findEntryKey('/app/routes/DesktopApp.svelte');
const bootstrapDynamicImports = new Set(manifest[entryKeys[0]]?.dynamicImports ?? []);

if (!bootstrapDynamicImports.has(desktopAppKey)) {
  throw new Error('The bootstrap entry must dynamically import DesktopApp directly.');
}

// DesktopApp is dynamically imported by the tiny bootstrap entry but awaited
// unconditionally before mount, so its complete static graph is part of the
// critical initial route for both leak detection and the size budget.
const initialKeys = new Set([...staticGraph(entryKeys[0]), ...staticGraph(desktopAppKey)]);
const initialDynamicTargets = new Set(
  [...initialKeys].flatMap((key) => manifest[key]?.dynamicImports ?? []),
);

const jsBytes = async (keys) => {
  let total = 0;
  for (const key of keys) {
    const file = manifest[key]?.file;
    if (typeof file === 'string' && file.endsWith('.js')) {
      total += (await stat(path.join(dist, file))).size;
    }
  }
  return total;
};

const initialBytes = await jsBytes(initialKeys);
if (initialBytes > INITIAL_JS_BUDGET_BYTES) {
  throw new Error(
    `Initial JS graph is ${initialBytes} bytes; budget is ${INITIAL_JS_BUDGET_BYTES}.`,
  );
}

const routeSizes = [];
for (const [screen, sourceSuffix] of Object.entries(LAZY_PAGE_ENTRIES)) {
  const key = findEntryKey(sourceSuffix);
  const chunk = manifest[key];

  if (chunk.isDynamicEntry !== true) {
    throw new Error(`Lazy page ${screen} is not emitted as a dynamic entry: ${key}`);
  }
  if (initialKeys.has(key)) {
    throw new Error(`Lazy page ${screen} leaked into the initial dependency graph: ${key}`);
  }
  if (!initialDynamicTargets.has(key)) {
    throw new Error(`Lazy page ${screen} is not dynamically reachable from the app shell: ${key}`);
  }

  const routeKeys = [...staticGraph(key)].filter((dependency) => !initialKeys.has(dependency));
  const routeBytes = await jsBytes(routeKeys);
  if (routeBytes > ROUTE_JS_BUDGET_BYTES) {
    throw new Error(
      `Lazy page ${screen} is ${routeBytes} bytes; budget is ${ROUTE_JS_BUDGET_BYTES}.`,
    );
  }
  routeSizes.push(`${screen} ${routeBytes}`);
}

const dynamicUpdateDialog = entries.find(
  ([key, chunk]) =>
    chunk.isDynamicEntry === true && normalizeKey(key).endsWith(UPDATE_DIALOG_ENTRY),
);
if (dynamicUpdateDialog) {
  throw new Error('AppUpdateDialog must be part of the eager application graph.');
}

console.log(`Bundle graph OK: initial ${initialBytes} bytes; lazy pages ${routeSizes.join(', ')}.`);
