import { describe, expect, it } from 'vitest';
import {
  encodePersistedGamesFilters,
  parsePersistedGamesFilters,
  type PersistedGamesFilters,
} from './filter-persistence';

describe('filter-persistence', () => {
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
        launchers: [],
        launcherOrder: [],
        searchQuery: '',
      });
    });

    it('normalizes legacy array payload libraries', () => {
      expectParsed(
        JSON.stringify([' LibraryAlpha ', 'LibraryAlpha', '', 'LibraryBeta', null, 15]),
        {
          libraries: ['LibraryAlpha', 'LibraryBeta'],
          launchers: [],
          launcherOrder: [],
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
          launchers: [],
          launcherOrder: [],
          searchQuery: 'alpha',
        },
      );
    });

    it('uses safe defaults when object fields are missing', () => {
      expectParsed(JSON.stringify({}), {
        libraries: [],
        launchers: [],
        launcherOrder: [],
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
          launchers: [],
          launcherOrder: [],
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
          launchers: [],
          launcherOrder: [],
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
          launchers: [],
          launcherOrder: [],
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
          launchers: [],
          launcherOrder: [],
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
            launchers: [],
            launcherOrder: [],
            searchQuery: 'alpha',
          }),
        ),
      ).toEqual({
        libraries: ['LibraryAlpha'],
        launchers: [],
        launcherOrder: [],
        searchQuery: 'alpha',
      } satisfies PersistedGamesFilters);
    });

    it('normalizes libraries before encoding', () => {
      expect(
        JSON.parse(
          encodePersistedGamesFilters({
            libraries: [' LibraryAlpha ', 'LibraryAlpha', '', 'LibraryBeta'],
            launchers: [],
            launcherOrder: [],
            searchQuery: '',
          }),
        ),
      ).toEqual({
        libraries: ['LibraryAlpha', 'LibraryBeta'],
        launchers: [],
        launcherOrder: [],
        searchQuery: '',
      } satisfies PersistedGamesFilters);
    });
  });
});

function createPersistedPayload(value: Record<string, unknown>): string {
  return JSON.stringify(value);
}

function expectParsed(value: string, expected: PersistedGamesFilters): void {
  expect(parsePersistedGamesFilters(value)).toEqual(expected satisfies PersistedGamesFilters);
}
