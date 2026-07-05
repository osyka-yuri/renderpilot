import { requireGameDetails, updateGameSummary } from '../desktop-state';
import { requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockSetGameFavorite(
  gameId: string,
  isFavorite: boolean,
): Promise<{ saved: boolean }> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'gameId');
    requireGameDetails(normalizedGameId);
    updateGameSummary(normalizedGameId, { is_favorite: isFavorite });
    return { saved: true };
  });
}

export function mockSetGameHidden(gameId: string, isHidden: boolean): Promise<{ saved: boolean }> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'gameId');
    requireGameDetails(normalizedGameId);
    updateGameSummary(normalizedGameId, { is_hidden: isHidden });
    return { saved: true };
  });
}
