import { describe, expect, it } from 'vitest';

import { createGameSummary, toGameCardViewModel } from '@entities/game';

import { createLauncherGroups, type CardStateContext } from './launcher-groups';

const CARD_CONTEXT: CardStateContext = {
  busy: false,
  hasManualCoverAction: false,
  pickDisabled: false,
  coversAutoFetchingIds: new Set(),
  menuOpenFor: null,
  actionMenuRefs: {},
  isCoverOperationBusy: () => false,
};

describe('createLauncherGroups', () => {
  it('groups cards and follows explicit then label-based launcher display order', () => {
    const games = [
      toGameCardViewModel(createGameSummary({ game_id: 'manual', launcher: 'Manual' }), 'en'),
      toGameCardViewModel(createGameSummary({ game_id: 'steam-1', launcher: 'Steam' }), 'en'),
      toGameCardViewModel(createGameSummary({ game_id: 'gog', launcher: 'Gog' }), 'en'),
      toGameCardViewModel(createGameSummary({ game_id: 'steam-2', launcher: 'Steam' }), 'en'),
    ];

    const groups = createLauncherGroups(games, ['Steam'], CARD_CONTEXT);

    expect(groups.map(({ launcher }) => launcher)).toEqual(['Steam', 'Gog', 'Manual']);
    expect(groups[0]?.cards.map(({ id }) => id)).toEqual(['steam-1', 'steam-2']);
  });
});
