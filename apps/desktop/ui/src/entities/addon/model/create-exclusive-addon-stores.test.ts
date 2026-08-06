import { describe, expect, it, vi } from 'vitest';

import { createExclusiveAddonStores } from './create-exclusive-addon-stores';

function makeStore(id: string) {
  return {
    id,
    busy: false,
    updateAvailable: false,
    load: vi.fn((_gameId: string) => Promise.resolve()),
    deactivate: vi.fn(),
    update: vi.fn((_gameId: string) => Promise.resolve('ok' as const)),
  };
}

describe('createExclusiveAddonStores', () => {
  it('reloads peer stores when exclusivity changes', () => {
    const luma = makeStore('luma');
    let exclusivityHandler: ((gameId: string) => void) | undefined;

    createExclusiveAddonStores({
      renodx: ({ onExclusivityChange }) => {
        exclusivityHandler = onExclusivityChange;
        return makeStore('renodx');
      },
      luma: () => luma,
    });

    exclusivityHandler?.('game-1');

    expect(luma.load).toHaveBeenCalledWith('game-1');
  });

  it('reloads only peers accepted by the key-aware predicate', () => {
    const luma = makeStore('luma');
    const third = makeStore('third');
    let exclusivityHandler: ((gameId: string) => void) | undefined;

    createExclusiveAddonStores(
      {
        renodx: ({ onExclusivityChange }) => {
          exclusivityHandler = onExclusivityChange;
          return makeStore('renodx');
        },
        luma: () => luma,
        third: () => third,
      },
      { shouldReloadPeer: (gameId, peer) => gameId === 'game-1' && peer === 'luma' },
    );

    exclusivityHandler?.('game-1');

    expect(luma.load).toHaveBeenCalledWith('game-1');
    expect(third.load).not.toHaveBeenCalled();
  });

  it('reloadPeers skips the excluded store', () => {
    const renodx = makeStore('renodx');
    const luma = makeStore('luma');

    const result = createExclusiveAddonStores({
      renodx: () => renodx,
      luma: () => luma,
    });

    result.reloadPeers('game-2', luma);

    expect(renodx.load).toHaveBeenCalledWith('game-2');

    expect(luma.load).not.toHaveBeenCalled();
  });
});
