import type { CoverArtworkResult } from '@entities/game';
import { requireGameDetails, updateGameSummary } from '../desktop-state';
import { normalizeCoverSourcePath, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockFetchGameCover(gameId: string): Promise<CoverArtworkResult> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');

    requireGameDetails(normalizedGameId);

    const updated_at_ms = Date.now();

    updateGameSummary(normalizedGameId, { cover_updated_at_ms: updated_at_ms });

    return {
      file_name: `cover-${normalizedGameId}-mock.png`,
      updated_at_ms,
    };
  });
}

export function mockClearGameCover(gameId: string): Promise<{ cleared: boolean }> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');

    requireGameDetails(normalizedGameId);
    updateGameSummary(normalizedGameId, { cover_updated_at_ms: null });

    return { cleared: true };
  });
}

export function mockSetGameCover(gameId: string, sourcePath: string): Promise<CoverArtworkResult> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');

    normalizeCoverSourcePath(sourcePath);
    requireGameDetails(normalizedGameId);

    const updated_at_ms = Date.now();

    updateGameSummary(normalizedGameId, { cover_updated_at_ms: updated_at_ms });

    return {
      file_name: `cover-${normalizedGameId}-picked.png`,
      updated_at_ms,
    };
  });
}
