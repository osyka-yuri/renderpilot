import { describe, expect, it } from 'vitest';
import {
  extractAvailableLibrariesFromCards,
  hasPartialLibrarySelection,
  intersectLibraries,
} from './library-filters';
import type { GameSummary } from './types';

function createGameSummary(libraryTags: readonly string[]): GameSummary {
  return {
    game_id: 'game-1',
    title: 'Game',
    launcher: 'Steam',
    platform: 'windows',
    runtime: 'dx12',
    install_path: 'C:/Games/Game',
    library_tags: [...libraryTags],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: 'safe',
    rollback_available: false,
    operation_count: 0,
    last_operation_status: null,
    cover_updated_at_ms: null,
    is_favorite: false,
    is_hidden: false,
  };
}

describe('library-filters', () => {
  describe('extractAvailableLibrariesFromCards', () => {
    it('collects normalized libraries and excludes unknown entries', () => {
      expect(
        extractAvailableLibrariesFromCards([
          createGameSummary([' steam ', 'unknown', 'dlss_super_resolution']),
          createGameSummary(['UNKNOWN', 'steam', 'amd_fsr']),
        ]),
      ).toEqual(['steam', 'dlss_super_resolution', 'amd_fsr']);
    });
  });

  describe('intersectLibraries', () => {
    it('returns an empty list when catalog is empty', () => {
      expect(intersectLibraries(['LibraryAlpha'], [])).toEqual([]);
    });

    it('keeps selected libraries that exist in the available catalog', () => {
      expect(
        intersectLibraries(['LibraryAlpha', 'LibraryGamma'], ['LibraryAlpha', 'LibraryBeta']),
      ).toEqual(['LibraryAlpha']);
    });

    it('uses selected libraries order', () => {
      expect(
        intersectLibraries(['LibraryBeta', 'LibraryAlpha'], ['LibraryAlpha', 'LibraryBeta']),
      ).toEqual(['LibraryBeta', 'LibraryAlpha']);
    });

    it('normalizes selected libraries by trimming, removing empty values, and deduplicating', () => {
      expect(
        intersectLibraries(
          [' LibraryBeta ', 'LibraryAlpha', 'LibraryAlpha', ''],
          ['LibraryAlpha', 'LibraryBeta'],
        ),
      ).toEqual(['LibraryBeta', 'LibraryAlpha']);
    });

    it('ignores duplicate values in available libraries', () => {
      expect(
        intersectLibraries(['LibraryAlpha', 'LibraryBeta'], ['LibraryAlpha', 'LibraryAlpha']),
      ).toEqual(['LibraryAlpha']);
    });
  });

  describe('hasPartialLibrarySelection', () => {
    it('does not report partial selection when catalog is empty', () => {
      expect(hasPartialLibrarySelection(['LibraryAlpha'], [])).toBe(false);
    });

    it('does not report partial selection when all available libraries are selected', () => {
      expect(
        hasPartialLibrarySelection(
          ['LibraryAlpha', 'LibraryBeta'],
          ['LibraryAlpha', 'LibraryBeta'],
        ),
      ).toBe(false);
    });

    it('reports partial selection when only part of available libraries is selected', () => {
      expect(hasPartialLibrarySelection(['LibraryAlpha'], ['LibraryAlpha', 'LibraryBeta'])).toBe(
        true,
      );
    });

    it('reports partial selection when selected libraries are empty and catalog is not empty', () => {
      expect(hasPartialLibrarySelection([], ['LibraryAlpha'])).toBe(true);
    });

    it('does not count unknown selected libraries as selected catalog values', () => {
      expect(
        hasPartialLibrarySelection(
          ['LibraryAlpha', 'LibraryUnknown'],
          ['LibraryAlpha', 'LibraryBeta'],
        ),
      ).toBe(true);
    });

    it('normalizes selected libraries before checking selection completeness', () => {
      expect(
        hasPartialLibrarySelection(
          [' LibraryAlpha ', 'LibraryBeta', 'LibraryBeta'],
          ['LibraryAlpha', 'LibraryBeta'],
        ),
      ).toBe(false);
    });
  });
});
