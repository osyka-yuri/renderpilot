import { isUnknownRecord, safeJsonParse } from '@shared/validation';
import { normalizeUniqueTrimmedStringsFromUnknown, trimToEmpty } from '@shared/text';
import { normalizeLibraryValues, normalizeLauncherValues } from '@entities/game';

const EMPTY_SEARCH_QUERY = '';

export type PersistedGamesFilters = {
  libraries: string[];
  launchers: string[];
  searchQuery: string;
};

export function normalizeSearchQuery(value: string): string {
  return trimToEmpty(value);
}

export function normalizePersistedGamesFilters(
  filters: PersistedGamesFilters,
): PersistedGamesFilters {
  return {
    libraries: normalizeLibraryValues(filters.libraries),
    launchers: normalizeLauncherValues(filters.launchers),
    searchQuery: normalizeSearchQuery(filters.searchQuery),
  };
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

function readPersistedGamesFilters(value: unknown): PersistedGamesFilters | null {
  if (Array.isArray(value)) {
    return {
      libraries: normalizeUniqueTrimmedStringsFromUnknown(value),
      launchers: [],
      searchQuery: EMPTY_SEARCH_QUERY,
    };
  }

  if (!isUnknownRecord(value)) {
    return null;
  }

  return {
    libraries: readPersistedStringList(value.libraries),
    launchers: readPersistedStringList(value.launchers),
    searchQuery: readPersistedSearchQuery(value.searchQuery),
  };
}

function readPersistedStringList(value: unknown): string[] {
  return Array.isArray(value) ? normalizeUniqueTrimmedStringsFromUnknown(value) : [];
}

function readPersistedSearchQuery(value: unknown): string {
  return typeof value === 'string' ? normalizeSearchQuery(value) : EMPTY_SEARCH_QUERY;
}
