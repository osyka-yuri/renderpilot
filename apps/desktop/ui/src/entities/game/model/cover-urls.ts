import { requireNonBlankString, requireValidTimestampMs } from '@shared/utils';

/** Windows WebView2 resolves registered scheme `rp-cover` under this origin. */
export const GAME_COVER_ORIGIN = 'http://rp-cover.localhost' as const;

export function gameCoverAssetSrc(gameId: string): string {
  const safeGameId = requireNonBlankString(gameId, 'gameId');
  return `${GAME_COVER_ORIGIN}/${encodeURIComponent(safeGameId)}`;
}

/** Cache-busting query for WebView after cover bytes change at the same protocol URL. */
export function gameCoverAssetSrcWithVersion(gameId: string, updatedAtMs: number): string {
  const base = gameCoverAssetSrc(gameId);
  const safeUpdatedAtMs = requireValidTimestampMs(updatedAtMs, 'updatedAtMs');
  return `${base}?v=${encodeURIComponent(String(safeUpdatedAtMs))}`;
}
