import { describe, expect, it } from 'vitest';
import { hasPartialLibrarySelection, intersectLibraries } from '../index';

describe('library-filters', () => {
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
