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
import { createOptiScalerStore, type OptiScalerStore } from '@features/optiscaler';
import { createRenoDxStore } from '@features/renodx';
import type { MutationSafetyTokens } from '@entities/addon';
import type { FileSafetyScope } from './create-file-safety-context.svelte';

import type { RunUpdateAllOptions } from './run-update-all';

type CreateGameAddonsContextOptions = {
  getGameId: () => string | null;
  getCapabilities: () => readonly AddonCapability[];
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  requireSafetyTokens?: (
    gameId: string,
    scope: FileSafetyScope,
  ) => Promise<MutationSafetyTokens | null>;
};

function normalizeOptionalGameId(gameId: string | null): string | null {
  if (gameId === null) {
    return null;
  }
  const normalized = normalizeSelectableGameId(gameId);
  return normalized.length > 0 ? normalized : null;
}

/** Page-owned add-on registry, activation policy, and aggregate state. */
export function createGameAddonsContext(options: CreateGameAddonsContextOptions) {
  let destroyed = false;
  const requireSafetyTokens = options.requireSafetyTokens;
  const gameId = $derived(normalizeOptionalGameId(options.getGameId()));
  const capabilities = $derived(canonicalAddonCapabilities(options.getCapabilities()));

  const exclusivePeerReload = {
    run: (_changedGameId: string) => undefined,
  };
  const optiscaler: OptiScalerStore = createOptiScalerStore({
    onAddonStateChange: (changedGameId) => {
      exclusivePeerReload.run(changedGameId);
    },
    onGameDetailsInvalidate: (changedGameId) => options.onGameDetailsInvalidate?.(changedGameId),
    requireSafetyTokens: requireSafetyTokens
      ? (changedGameId) => requireSafetyTokens(changedGameId, 'game')
      : undefined,
  });

  async function invalidateAfterPeerMutation(changedGameId: string): Promise<void> {
    await options.onGameDetailsInvalidate?.(changedGameId);
    if (!destroyed && gameId !== null && areSameGameIds(changedGameId, gameId)) {
      await optiscaler.load(changedGameId);
    }
  }

  const exclusiveStores = createExclusiveAddonStores(
    {
      renodx: ({ onExclusivityChange }) =>
        createRenoDxStore({
          onExclusivityChange,
          onGameDetailsInvalidate: invalidateAfterPeerMutation,
          requireSafetyTokens: options.requireSafetyTokens,
        }),
      luma: ({ onExclusivityChange }) =>
        createLumaStore({
          onExclusivityChange,
          onGameDetailsInvalidate: invalidateAfterPeerMutation,
          requireSafetyTokens: options.requireSafetyTokens,
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
  const stores = exclusiveStores.stores;
  exclusivePeerReload.run = (changedGameId) => {
    exclusiveStores.reloadPeers(changedGameId);
  };

  const storesByCapability = {
    renodx: stores.renodx,
    luma: stores.luma,
    optiscaler,
  };
  const storeEntries = ALL_ADDON_CAPABILITIES.map((capability) => ({
    capability,
    store: storesByCapability[capability],
  }));
  const enabledStores = $derived(capabilities.map((capability) => storesByCapability[capability]));
  const updateCount = $derived(
    capabilities.filter((capability) => storesByCapability[capability].updateAvailable).length,
  );
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
