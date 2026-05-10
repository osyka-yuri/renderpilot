import type { GameCard } from '@shared/api/types';
import {
  normalizeUniqueTrimmedStrings,
  normalizeUniqueTrimmedStringsFromUnknown,
  trimToEmpty,
} from '@shared/utils/normalize';

/** SQLite `settings` key; keep stable across releases. */
export const GAMES_FILTERS_CATALOG_SETTING_KEY = 'games_filters_v3';

const EMPTY_SEARCH_QUERY = '';

type UnknownRecord = Record<string, unknown>;

export type PersistedGamesFilters = {
  libraries: string[];
  searchQuery: string;
};

export function normalizeLibraryValues(values: readonly string[]): string[] {
  return normalizeUniqueTrimmedStrings(values);
}

export function normalizeSearchQuery(value: string): string {
  return trimToEmpty(value);
}

export function normalizePersistedGamesFilters(
  filters: PersistedGamesFilters,
): PersistedGamesFilters {
  return {
    libraries: normalizeLibraryValues(filters.libraries),
    searchQuery: normalizeSearchQuery(filters.searchQuery),
  };
}

export function extractAvailableLibrariesFromCards(cards: readonly GameCard[]): string[] {
  const libraries: string[] = [];

  for (const card of cards) {
    libraries.push(...card.library_tags);
  }

  return normalizeLibraryValues(libraries);
}

/** Keep only values still present in the catalog union. */
export function intersectLibraries(
  selection: readonly string[],
  available: readonly string[],
): string[] {
  return intersectNormalizedLibraries(
    normalizeLibraryValues(selection),
    normalizeLibraryValues(available),
  );
}

export function shallowStringArrayEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left === right ||
    (left.length === right.length && left.every((value, index) => value === right[index]))
  );
}

export function hasPartialLibrarySelection(
  selectedLibraries: readonly string[],
  availableLibraryValues: readonly string[],
): boolean {
  const availableLibraries = normalizeLibraryValues(availableLibraryValues);

  if (availableLibraries.length === 0) {
    return false;
  }

  const selectedAvailableLibraries = intersectNormalizedLibraries(
    normalizeLibraryValues(selectedLibraries),
    availableLibraries,
  );

  return selectedAvailableLibraries.length < availableLibraries.length;
}

export function parsePersistedGamesFilters(value: string | null): PersistedGamesFilters | null {
  if (value === null) {
    return null;
  }

  return readPersistedGamesFilters(safeJsonParse(value));
}

export function encodePersistedGamesFilters(filters: PersistedGamesFilters): string {
  const normalizedFilters = normalizePersistedGamesFilters(filters);

  return JSON.stringify(normalizedFilters);
}

export function buildGameCardsQueryKey(
  searchQuery: string,
  selectedLibraries: readonly string[],
): string {
  return JSON.stringify({
    searchQuery,
    selectedLibraries,
  });
}

function intersectNormalizedLibraries(
  selection: readonly string[],
  available: readonly string[],
): string[] {
  if (available.length === 0) {
    return [];
  }

  const allowedLibraries = new Set(available);

  return selection.filter((library) => allowedLibraries.has(library));
}

function readPersistedGamesFilters(value: unknown): PersistedGamesFilters | null {
  if (Array.isArray(value)) {
    return {
      libraries: normalizeUniqueTrimmedStringsFromUnknown(value),
      searchQuery: EMPTY_SEARCH_QUERY,
    };
  }

  if (!isUnknownRecord(value)) {
    return null;
  }

  return {
    libraries: readPersistedLibraries(value.libraries),
    searchQuery: readPersistedSearchQuery(value.searchQuery),
  };
}

function readPersistedLibraries(value: unknown): string[] {
  return Array.isArray(value) ? normalizeUniqueTrimmedStringsFromUnknown(value) : [];
}

function readPersistedSearchQuery(value: unknown): string {
  return typeof value === 'string' ? normalizeSearchQuery(value) : EMPTY_SEARCH_QUERY;
}

function safeJsonParse(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function isUnknownRecord(value: unknown): value is UnknownRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
