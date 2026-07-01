import type { RenoDxStore } from './create-renodx-store.svelte';

export type RenoDxCardView =
  | 'loading'
  | 'load-error'
  | 'installed'
  | 'external'
  | 'native-hdr'
  | 'blacklisted'
  | 'unsupported'
  | 'incompatible'
  | 'installable'
  | 'unavailable';

/** The subset of store state `getCardView` reads, so it's testable without a full store. */
export type CardViewSource = Pick<
  RenoDxStore,
  | 'loading'
  | 'loaded'
  | 'loadError'
  | 'isInstalled'
  | 'isExternal'
  | 'isNativeHdr'
  | 'isBlacklisted'
  | 'isUnsupported'
  | 'isIncompatible'
  | 'isInstallable'
>;

/**
 * Resolves which top-level view the RenoDX card renders. Order is priority,
 * highest first: a load in progress or a load error always wins over any
 * availability outcome, and the outcome kinds are otherwise mutually
 * exclusive (the backend reports exactly one).
 */
export function getCardView(store: CardViewSource): RenoDxCardView {
  if (store.loading && !store.loaded) {
    return 'loading';
  }

  if (store.loadError) {
    return 'load-error';
  }

  if (store.isInstalled) {
    return 'installed';
  }

  if (store.isExternal) {
    return 'external';
  }

  if (store.isNativeHdr) {
    return 'native-hdr';
  }

  if (store.isBlacklisted) {
    return 'blacklisted';
  }

  if (store.isUnsupported) {
    return 'unsupported';
  }

  if (store.isIncompatible) {
    return 'incompatible';
  }

  if (store.isInstallable) {
    return 'installable';
  }

  return 'unavailable';
}
