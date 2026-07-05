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
  it('reloads peer stores when exclusivity changes', () => {
    const peer = makeStore('peer');
    let exclusivityHandler: ((gameId: string) => void) | undefined;

    createExclusiveAddonStores({
      renodx: ({ onExclusivityChange }) => {
        exclusivityHandler = onExclusivityChange;
        return makeStore('renodx');
      },
      peer: () => peer,
    });

    exclusivityHandler?.('game-1');

    expect(peer.load).toHaveBeenCalledWith('game-1');
  });

  it('skips peer reload when shouldReloadPeers returns false', () => {
    const peer = makeStore('peer');
    let exclusivityHandler: ((gameId: string) => void) | undefined;

    createExclusiveAddonStores(
      {
        renodx: ({ onExclusivityChange }) => {
          exclusivityHandler = onExclusivityChange;
          return makeStore('renodx');
        },
        peer: () => peer,
      },
      { shouldReloadPeers: () => false },
    );

    exclusivityHandler?.('game-1');

    expect(peer.load).not.toHaveBeenCalled();
  });

  it('reloadPeers skips the excluded store', () => {
    const renodx = makeStore('renodx');
    const peer = makeStore('peer');

    const result = createExclusiveAddonStores({
      renodx: () => renodx,
      peer: () => peer,
    });

    result.reloadPeers('game-2', peer);

    expect(renodx.load).toHaveBeenCalledWith('game-2');
    expect(peer.load).not.toHaveBeenCalled();
  });
});
