import { untrack } from 'svelte';

import { track } from '@shared/reactivity';
import { createExclusiveAddonStores } from '@entities/addon';
import {
  ALL_ADDON_CAPABILITIES,
  areSameGameIds,
  canonicalAddonCapabilities,
  normalizeSelectableGameId,
  type AddonCapability,
} from '@entities/game';
import { createLumaStore } from '@features/luma';
import { createRenoDxStore } from '@features/renodx';
import type { MutationSafetyTokens } from '@entities/addon';
import type { FileSafetyScope } from './create-file-safety-context.svelte';

import type { RunUpdateAllOptions } from './run-update-all';

type CreateGameAddonsContextOptions = {
  getGameId: () => string | null;
  getCapabilities: () => readonly AddonCapability[];
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  requireSafetyTokens?: (gameId: string, scope: FileSafetyScope) => Promise<MutationSafetyTokens>;
  onSafetyContextError?: (error: unknown, scope: FileSafetyScope) => void | Promise<void>;
};

function normalizeOptionalGameId(gameId: string | null): string | null {
  if (gameId === null) {
    return null;
  }
  const normalized = normalizeSelectableGameId(gameId);
  return normalized.length > 0 ? normalized : null;
}

/** Page-owned RenoDX/Luma registry, activation policy, and aggregate state. */
export function createGameAddonsContext(options: CreateGameAddonsContextOptions) {
  let destroyed = false;
  const gameId = $derived(normalizeOptionalGameId(options.getGameId()));
  const capabilities = $derived(canonicalAddonCapabilities(options.getCapabilities()));

  const { stores } = createExclusiveAddonStores(
    {
      renodx: ({ onExclusivityChange }) =>
        createRenoDxStore({
          onExclusivityChange,
          onGameDetailsInvalidate: options.onGameDetailsInvalidate,
          requireSafetyTokens: options.requireSafetyTokens,
          onSafetyContextError: options.onSafetyContextError,
        }),
      luma: ({ onExclusivityChange }) =>
        createLumaStore({
          onExclusivityChange,
          onGameDetailsInvalidate: options.onGameDetailsInvalidate,
          requireSafetyTokens: options.requireSafetyTokens,
          onSafetyContextError: (error) => options.onSafetyContextError?.(error, 'game'),
        }),
    },
    {
      shouldReloadPeer: (changedGameId, peer) =>
        !destroyed &&
        gameId !== null &&
        areSameGameIds(changedGameId, gameId) &&
        capabilities.includes(peer),
    },
  );

  const storesByCapability = {
    renodx: stores.renodx,
    luma: stores.luma,
  } satisfies Record<AddonCapability, (typeof stores)[keyof typeof stores]>;
  const storeEntries = ALL_ADDON_CAPABILITIES.map((capability) => ({
    capability,
    store: storesByCapability[capability],
  }));
  const enabledStores = $derived(capabilities.map((capability) => storesByCapability[capability]));
  const updateCount = $derived(enabledStores.filter((store) => store.updateAvailable).length);
  const busy = $derived(enabledStores.some((store) => store.busy));
  const addonUpdates = $derived<RunUpdateAllOptions['addonUpdates']>(
    capabilities
      .map((step) => ({ step, store: storesByCapability[step] }))
      .filter(({ store }) => store.updateAvailable),
  );

  // A primitive key prevents unrelated same-game details updates from repeating probes.
  const activationSignature = $derived(JSON.stringify([gameId, ...capabilities]));
  $effect(() => {
    track(activationSignature);
    untrack(() => {
      if (destroyed) {
        return;
      }
      for (const { capability, store } of storeEntries) {
        if (gameId !== null && capabilities.includes(capability)) {
          void store.load(gameId);
        } else {
          store.deactivate();
        }
      }
    });
  });

  function isEnabled(capability: AddonCapability): boolean {
    return capabilities.includes(capability);
  }

  function destroy(): void {
    if (destroyed) {
      return;
    }
    destroyed = true;
    for (const { store } of storeEntries) {
      store.deactivate();
    }
  }

  return {
    stores: storesByCapability,
    get capabilities() {
      return capabilities;
    },
    get busy() {
      return busy;
    },
    get updateCount() {
      return updateCount;
    },
    get addonUpdates() {
      return addonUpdates;
    },
    isEnabled,
    destroy,
  };
}
