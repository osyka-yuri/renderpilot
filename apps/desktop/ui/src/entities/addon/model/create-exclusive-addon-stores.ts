type ReloadableAddonStore = {
  load(gameId: string): Promise<void>;
};

type AddonStoreFactory<T extends ReloadableAddonStore> = (opts: {
  onExclusivityChange: (gameId: string) => void;
}) => T;

/**
 * Creates mutually exclusive per-game add-on stores and wires peer reloads
 * after install/uninstall mutations flip the exclusivity block.
 */
export function createExclusiveAddonStores<T extends Record<string, ReloadableAddonStore>>(
  factories: { [K in keyof T]: AddonStoreFactory<T[K]> },
  options?: {
    shouldReloadPeer?: (changedGameId: string, peer: keyof T) => boolean;
  },
): {
  stores: T;
  reloadPeers(gameId: string, exclude?: ReloadableAddonStore): void;
} {
  const storeEntries = Object.keys(factories) as (keyof T)[];
  const stores = {} as T;

  const reloadPeers = (gameId: string, exclude?: ReloadableAddonStore): void => {
    for (const key of storeEntries) {
      const store = stores[key];
      if (store !== exclude && (options?.shouldReloadPeer?.(gameId, key) ?? true)) {
        void store.load(gameId);
      }
    }
  };

  for (const key of storeEntries) {
    const store = factories[key]({
      onExclusivityChange: (changedGameId) => {
        reloadPeers(changedGameId, store);
      },
    });
    stores[key] = store;
  }

  return { stores, reloadPeers };
}
