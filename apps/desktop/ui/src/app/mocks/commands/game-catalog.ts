import type {
  GameRemovalAssessment,
  NvapiClaimAbandonmentAuthorization,
  RemoveGameFromCatalogResult,
} from '@entities/game';
import { clearGameExecutableOverride, mockState } from '../desktop-state';
import { requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockAssessGameRemoval(gameId: string): Promise<GameRemovalAssessment> {
  return resolveMock(() => {
    const id = requireNonEmptyText(gameId, 'game id');

    if (id.includes('diverged')) {
      return {
        gameId: id,
        canRemoveAutomatically: false,
        nvapiAssessment: {
          requiresUserAuthorization: true,
          blockedReason: null,
          claims: [
            {
              targetId: 'Base Profile',
              settingId: 0x10b3292c,
              settingName: 'DLSS Preset',
              status: 'diverged',
              recordedExpected: { present: true, value: 1 },
              recordedOriginal: { present: false, value: null },
              observedLive: { present: true, value: 2 },
              disposition: 'requiresAbandonmentAuthorization',
            },
          ],
        },
      };
    }

    if (id.includes('blocked')) {
      return {
        gameId: id,
        canRemoveAutomatically: false,
        nvapiAssessment: {
          requiresUserAuthorization: false,
          blockedReason: 'executable witness changed: rebound to other profile',
          claims: [],
        },
      };
    }

    return {
      gameId: id,
      canRemoveAutomatically: true,
      nvapiAssessment: {
        requiresUserAuthorization: false,
        blockedReason: null,
        claims: [],
      },
    };
  });
}

export function mockRemoveGameFromCatalog(
  gameId: string,
  authorization?: NvapiClaimAbandonmentAuthorization | null,
): Promise<RemoveGameFromCatalogResult> {
  return resolveMock(() => {
    const id = requireNonEmptyText(gameId, 'game id');
    const card = mockState.games.find((game) => game.game_id === id);
    if (!card) {
      throw new Error(`Mock preview could not find game ${id}.`);
    }
    if (!card.can_remove_from_catalog) {
      throw new Error('Launcher-managed games cannot be removed from the catalog.');
    }
    if (id.includes('diverged') && (!authorization || authorization.claims.length === 0)) {
      throw new Error('Claim abandonment authorization required for diverged settings.');
    }
    mockState.games = mockState.games.filter((game) => game.game_id !== id);
    mockState.detailsByGameId.delete(id);
    clearGameExecutableOverride(id);
    mockState.componentBaselinesByGameId.delete(id);
    mockState.autoGameIds.delete(id);
    for (const [installPath, mappedId] of mockState.manualGameIdByInstallPath) {
      if (mappedId === id) {
        mockState.manualGameIdByInstallPath.delete(installPath);
      }
    }

    return { gameId: id };
  });
}
