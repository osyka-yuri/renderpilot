import type { AddonStoreCore } from './create-addon-store.svelte';
import type { AddonInstallStateBase, FreshnessSource } from './store-helpers';

/**
 * Live getters for the shared `createAddonStore` surface.
 *
 * Tool stores must merge these with `mergeAddonApis` (which preserves getter
 * descriptors) so Svelte reactivity keeps tracking `core.*` reads.
 */
export function addonCoreApi<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
>(core: AddonStoreCore<TState, TUpdateReport>) {
  return {
    get state() {
      return core.state;
    },
    get loading() {
      return core.loading;
    },
    get loaded() {
      return core.loaded;
    },
    get busy() {
      return core.busy;
    },
    get loadError() {
      return core.loadError;
    },
    get updateStatus() {
      return core.updateStatus;
    },
    get updateProbing() {
      return core.updateProbing;
    },
    get freshness() {
      return core.freshness;
    },
    get lastCheckedAt() {
      return core.lastCheckedAt;
    },
    get isInstalled() {
      return core.isInstalled;
    },
    get addonDated() {
      return core.addonDated;
    },
    get installedAt() {
      return core.installedAt;
    },
    get updatedAt() {
      return core.updatedAt;
    },
    get addonUpdate() {
      return core.addonUpdate;
    },
    get hostUpdate() {
      return core.hostUpdate;
    },
    get updateAvailable() {
      return core.updateAvailable;
    },
    get safetyContextError() {
      return core.safetyContextError;
    },
    get requestToken() {
      return core.requestToken;
    },
    deactivate: core.deactivate,
  };
}

/**
 * Merges property descriptors so live getters from every source object (core
 * api, shared outcome/host api, tool-specific fields, …) stay live (plain
 * object spread would freeze getter values at merge time).
 */
export function mergeAddonApis<A extends object, B extends object>(a: A, b: B): A & B;
export function mergeAddonApis<A extends object, B extends object, C extends object>(
  a: A,
  b: B,
  c: C,
): A & B & C;
export function mergeAddonApis<
  A extends object,
  B extends object,
  C extends object,
  D extends object,
>(a: A, b: B, c: C, d: D): A & B & C & D;
export function mergeAddonApis(...apis: object[]): object {
  const descriptors = apis.reduce<PropertyDescriptorMap>(
    (merged, api) => ({ ...merged, ...Object.getOwnPropertyDescriptors(api) }),
    {},
  );
  return Object.defineProperties({}, descriptors);
}
