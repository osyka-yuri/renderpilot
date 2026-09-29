import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  GAMES_VIEW_MODE_STORAGE_KEY,
  persistGamesViewMode,
  readStoredGamesViewMode,
} from './games-view-mode-storage';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('games view mode storage', () => {
  it('defaults to cards for missing or unrecognized values', () => {
    const storage = new Map<string, string>();
    stubStorage(storage);

    expect(readStoredGamesViewMode()).toBe('cards');
    storage.set(GAMES_VIEW_MODE_STORAGE_KEY, 'unknown');
    expect(readStoredGamesViewMode()).toBe('cards');
  });

  it('persists and restores the list preference', () => {
    const storage = new Map<string, string>();
    stubStorage(storage);

    persistGamesViewMode('list');
    expect(storage.get(GAMES_VIEW_MODE_STORAGE_KEY)).toBe('list');
    expect(readStoredGamesViewMode()).toBe('list');
  });

  it('keeps the default when browser storage is unavailable', () => {
    vi.stubGlobal('window', {
      get localStorage(): never {
        throw new Error('Storage blocked');
      },
    });

    expect(readStoredGamesViewMode()).toBe('cards');
    expect(() => {
      persistGamesViewMode('list');
    }).not.toThrow();
  });
});

function stubStorage(values: Map<string, string>): void {
  vi.stubGlobal('window', {
    localStorage: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => {
        values.set(key, value);
      },
    },
  });
}
