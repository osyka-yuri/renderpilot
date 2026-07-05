import { describe, expect, it, vi } from 'vitest';

import { createExclusiveAddonStores } from './create-exclusive-addon-stores';

function makeStore(id: string) {
  return {
    id,
    busy: false,
    updateAvailable: false,
    load: vi.fn((_gameId: string) => Promise.resolve()),
    update: vi.fn((_gameId: string) => Promise.resolve(true)),
  };
}

describe('createExclusiveAddonStores', () => {
  it('does not reload the excluded store when no peer tool is registered', () => {
    const renodx = makeStore('renodx');

    const result = createExclusiveAddonStores({
      renodx: () => renodx,
    });

    result.reloadPeers('game-2', renodx);

    expect(renodx.load).not.toHaveBeenCalled();
  });
});
