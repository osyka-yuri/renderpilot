import { getCardView as sharedGetCardView } from '@entities/addon';
import type { BaseCardView } from '@entities/addon';

import type { RenoDxStore } from './create-renodx-store.svelte';

export type RenoDxCardView = BaseCardView | 'external' | 'native-hdr';

/** The subset of store state `getCardView` reads, so it's testable without a full store. */
export type CardViewSource = Pick<
  RenoDxStore,
  | 'loading'
  | 'loaded'
  | 'loadError'
  | 'isInstalled'
  | 'isBlockedByOtherAddon'
  | 'isExternal'
  | 'isNativeHdr'
  | 'isBlacklisted'
  | 'isUnsupported'
  | 'isIncompatible'
  | 'isInstallable'
>;

/**
 * Resolves which top-level view the RenoDX card renders. Order is priority,
 * highest first: a retained load error wins while its retry is in progress;
 * otherwise the initial load state wins over any availability outcome. The
 * outcome kinds are otherwise mutually
 * exclusive (the backend reports exactly one).
 */
export function getCardView(store: CardViewSource): RenoDxCardView {
  return sharedGetCardView(store, () => {
    if (store.isExternal) {
      return 'external';
    }
    if (store.isNativeHdr) {
      return 'native-hdr';
    }
    return null;
  });
}
