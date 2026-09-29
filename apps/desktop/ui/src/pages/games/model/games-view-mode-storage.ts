import type { GamesViewMode } from '@widgets/games-catalog';

export const GAMES_VIEW_MODE_STORAGE_KEY = 'renderpilot.games.view-mode';

const DEFAULT_VIEW_MODE: GamesViewMode = 'cards';

export function readStoredGamesViewMode(): GamesViewMode {
  const storage = getLocalStorage();
  if (storage === null) {
    return DEFAULT_VIEW_MODE;
  }

  try {
    return storage.getItem(GAMES_VIEW_MODE_STORAGE_KEY) === 'list' ? 'list' : DEFAULT_VIEW_MODE;
  } catch {
    return DEFAULT_VIEW_MODE;
  }
}

export function persistGamesViewMode(mode: GamesViewMode): void {
  const storage = getLocalStorage();
  if (storage === null) {
    return;
  }

  try {
    storage.setItem(GAMES_VIEW_MODE_STORAGE_KEY, mode);
  } catch {
    // The view remains usable when browser storage is unavailable.
  }
}

function getLocalStorage(): Storage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
