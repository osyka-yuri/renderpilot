export type AddonStoreLike = {
  load(gameId: string): Promise<void>;
  // Both tool stores resolve `update` with a success boolean; callers that
  // only need to know a mutation ran (e.g. "update all") can ignore it.
  update(gameId: string): Promise<unknown>;
  busy: boolean;
  updateAvailable: boolean;
};

type AddonStoreFactory<T extends AddonStoreLike> = (opts: {
  onExclusivityChange: (gameId: string) => void;
}) => T;

/**
 * Creates mutually exclusive per-game add-on stores and wires peer reloads
 * after install/uninstall mutations flip the exclusivity block.
 */
export function createExclusiveAddonStores<T extends Record<string, AddonStoreLike>>(
  factories: { [K in keyof T]: AddonStoreFactory<T[K]> },
  options?: {
    shouldReloadPeers?: (changedGameId: string) => boolean;
  },
): {
  stores: T;
  list: AddonStoreLike[];
  reloadPeers(gameId: string, exclude?: AddonStoreLike): void;
} {
  const storeEntries = Object.keys(factories) as (keyof T)[];
  const stores = {} as T;
  const list: AddonStoreLike[] = [];

  const reloadPeers = (gameId: string, exclude?: AddonStoreLike): void => {
    for (const store of list) {
      if (store !== exclude) {
        void store.load(gameId);
      }
    }
  };

  for (const key of storeEntries) {
    const store = factories[key]({
      onExclusivityChange: (changedGameId) => {
        if (options?.shouldReloadPeers?.(changedGameId) ?? true) {
          reloadPeers(changedGameId, store);
        }
      },
    });
    stores[key] = store;
    list.push(store);
  }

  return { stores, list, reloadPeers };
}
