import { describe, expect, it } from 'vitest';
import {
  encodePersistedGamesFilters,
  hasPartialLibrarySelection,
  intersectLibraries,
  parsePersistedGamesFilters,
  type PersistedGamesFilters,
} from '@features/games/games-screen-filters';

describe('games-screen-filters', () => {
  describe('parsePersistedGamesFilters', () => {
    it('returns null when persisted value is null', () => {
      expect(parsePersistedGamesFilters(null)).toBeNull();
    });

    it.each([
      ['invalid json', '{not-valid-json'],
      ['string payload', '"just-a-string"'],
      ['number payload', '42'],
      ['boolean payload', 'true'],
      ['null payload', 'null'],
    ])('returns null for %s', (_, payload) => {
      expect(parsePersistedGamesFilters(payload)).toBeNull();
    });

    it('parses legacy array payload as libraries list', () => {
      expectParsed('["LibraryAlpha","LibraryBeta"]', {
        libraries: ['LibraryAlpha', 'LibraryBeta'],
        searchQuery: '',
      });
    });

    it('normalizes legacy array payload libraries', () => {
      expectParsed(
        JSON.stringify([' LibraryAlpha ', 'LibraryAlpha', '', 'LibraryBeta', null, 15]),
        {
          libraries: ['LibraryAlpha', 'LibraryBeta'],
          searchQuery: '',
        },
      );
    });

    it('parses object payload fields', () => {
      expectParsed(
        createPersistedPayload({
          libraries: ['LibraryAlpha'],
          searchQuery: 'alpha',
        }),
        {
          libraries: ['LibraryAlpha'],
          searchQuery: 'alpha',
        },
      );
    });

    it('uses safe defaults when object fields are missing', () => {
      expectParsed(JSON.stringify({}), {
        libraries: [],
        searchQuery: '',
      });
    });

    it('normalizes malformed object payload fields', () => {
      expectParsed(
        createPersistedPayload({
          libraries: ['LibraryAlpha', 15, null],
          searchQuery: ['alpha'],
        }),
        {
          libraries: ['LibraryAlpha'],
          searchQuery: '',
        },
      );
    });

    it('normalizes persisted libraries by trimming, removing empty values, and deduplicating', () => {
      expectParsed(
        createPersistedPayload({
          libraries: [' LibraryAlpha ', 'LibraryAlpha', '', 'LibraryBeta'],
          searchQuery: '',
        }),
        {
          libraries: ['LibraryAlpha', 'LibraryBeta'],
          searchQuery: '',
        },
      );
    });

    it('normalizes non-array libraries field to an empty list', () => {
      expectParsed(
        createPersistedPayload({
          libraries: 'LibraryAlpha',
          searchQuery: 'alpha',
        }),
        {
          libraries: [],
          searchQuery: 'alpha',
        },
      );
    });

    it('normalizes non-string searchQuery field to an empty string', () => {
      expectParsed(
        createPersistedPayload({
          libraries: ['LibraryAlpha'],
          searchQuery: 15,
        }),
        {
          libraries: ['LibraryAlpha'],
          searchQuery: '',
        },
      );
    });
  });

  describe('encodePersistedGamesFilters', () => {
    it('encodes persisted filters as a stable JSON object payload', () => {
      expect(
        JSON.parse(
          encodePersistedGamesFilters({
            libraries: ['LibraryAlpha'],
            searchQuery: 'alpha',
          }),
        ),
      ).toEqual({
        libraries: ['LibraryAlpha'],
        searchQuery: 'alpha',
      } satisfies PersistedGamesFilters);
    });

    it('normalizes libraries before encoding', () => {
      expect(
        JSON.parse(
          encodePersistedGamesFilters({
            libraries: [' LibraryAlpha ', 'LibraryAlpha', '', 'LibraryBeta'],
            searchQuery: '',
          }),
        ),
      ).toEqual({
        libraries: ['LibraryAlpha', 'LibraryBeta'],
        searchQuery: '',
      } satisfies PersistedGamesFilters);
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

function createPersistedPayload(value: Record<string, unknown>): string {
  return JSON.stringify(value);
}

function expectParsed(value: string, expected: PersistedGamesFilters): void {
  expect(parsePersistedGamesFilters(value)).toEqual(expected satisfies PersistedGamesFilters);
}
