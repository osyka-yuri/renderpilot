import { getCardView as sharedGetCardView } from '@entities/addon';
import type { BaseCardView } from '@entities/addon';

import type { LumaStore } from './create-luma-store.svelte';

export type LumaCardView = BaseCardView | 'unmanaged-present';

/** The subset of store state `getCardView` reads, so it's testable without a full store. */
export type CardViewSource = Pick<
  LumaStore,
  | 'loading'
  | 'loaded'
  | 'loadError'
  | 'isInstalled'
  | 'isBlockedByOtherAddon'
  | 'isUnmanagedPresent'
  | 'isBlacklisted'
  | 'isUnsupported'
  | 'isIncompatible'
  | 'isInstallable'
>;

/**
 * Resolves which top-level view the Luma card renders. Order is priority,
 * highest first: a retained load error wins while its retry is in progress;
 * otherwise the initial load state wins over any availability outcome. The
 * outcome kinds are otherwise mutually
 * exclusive (the backend reports exactly one).
 */
export function getCardView(store: CardViewSource): LumaCardView {
  return sharedGetCardView(store, () => (store.isUnmanagedPresent ? 'unmanaged-present' : null));
}
