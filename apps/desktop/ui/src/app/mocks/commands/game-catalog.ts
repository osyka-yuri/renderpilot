import type { RemoveGameFromCatalogResult } from '@entities/game';
import { clearGameExecutableOverride, mockState } from '../desktop-state';
import { requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockRemoveGameFromCatalog(gameId: string): Promise<RemoveGameFromCatalogResult> {
  return resolveMock(() => {
    const id = requireNonEmptyText(gameId, 'game id');
    const card = mockState.games.find((game) => game.game_id === id);
    if (!card) {
      throw new Error(`Mock preview could not find game ${id}.`);
    }
    if (!card.can_remove_from_catalog) {
      throw new Error('Launcher-managed games cannot be removed from the catalog.');
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
