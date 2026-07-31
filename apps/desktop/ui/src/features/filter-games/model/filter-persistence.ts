import { isUnknownRecord, safeJsonParse } from '@shared/validation';
import { normalizeUniqueTrimmedStringsFromUnknown, trimToEmpty } from '@shared/text';
import {
  intersectLibraries,
  normalizeAddonCapabilities,
  normalizeLibraryValues,
  normalizeLauncherValues,
} from '@entities/game';
import { ALL_KNOWN_LIBRARIES } from '@shared/graphics';

const EMPTY_SEARCH_QUERY = '';
/**
 * Frozen default selection from the last application version without Xiph.
 * This migration fingerprint must not grow with future technology additions.
 */
const PRE_XIPH_DEFAULT_LIBRARIES = [
  'dlss_super_resolution',
  'dlss_frame_generation',
  'dlss_ray_reconstruction',
  'nvidia_streamline',
  'intel_xess',
  'intel_xefg',
  'intel_xell',
  'amd_fsr',
  'amd_fsr_frame_generation',
  'amd_fsr_ray_regeneration',
  'direct_storage',
  'microsoft_dxc',
  'd3d12_agility',
  'openvr',
] as const;

export type PersistedGamesFilters = {
  libraries: string[];
  addons?: string[] | null;
  launchers: string[];
  launcherOrder: string[];
  searchQuery: string;
  showHidden: boolean;
  favoritesOnly: boolean;
};

export function normalizeSearchQuery(value: string): string {
  return trimToEmpty(value);
}

export function normalizePersistedGamesFilters(
  filters: PersistedGamesFilters,
): PersistedGamesFilters {
  return {
    libraries: normalizeLibraryValues(filters.libraries),
    ...(filters.addons == null ? {} : { addons: normalizeAddonCapabilities(filters.addons) }),
    launchers: normalizeLauncherValues(filters.launchers),
    launcherOrder: normalizeLauncherValues(filters.launcherOrder),
    searchQuery: normalizeSearchQuery(filters.searchQuery),
    showHidden: filters.showHidden,
    favoritesOnly: filters.favoritesOnly,
  };
}

export function parsePersistedGamesFilters(value: string | null): PersistedGamesFilters | null {
  if (value === null) {
    return null;
  }

  const filters = readPersistedGamesFilters(safeJsonParse(value));

  return filters === null ? null : migratePreXiphDefaultSelection(filters);
}

export function encodePersistedGamesFilters(filters: PersistedGamesFilters): string {
  const normalizedFilters = normalizePersistedGamesFilters(filters);

  return JSON.stringify(normalizedFilters);
}

function readPersistedGamesFilters(value: unknown): PersistedGamesFilters | null {
  if (Array.isArray(value)) {
    return {
      libraries: normalizeUniqueTrimmedStringsFromUnknown(value),
      addons: null,
      launchers: [],
      launcherOrder: [],
      searchQuery: EMPTY_SEARCH_QUERY,
      showHidden: false,
      favoritesOnly: false,
    };
  }

  if (!isUnknownRecord(value)) {
    return null;
  }

  return {
    libraries: readPersistedStringList(value.libraries),
    addons: 'addons' in value ? readPersistedStringList(value.addons) : null,
    launchers: readPersistedStringList(value.launchers),
    launcherOrder: readPersistedStringList(value.launcherOrder),
    searchQuery: readPersistedSearchQuery(value.searchQuery),
    showHidden: readPersistedBoolean(value.showHidden),
    favoritesOnly: readPersistedBoolean(value.favoritesOnly),
  };
}

function readPersistedBoolean(value: unknown): boolean {
  return typeof value === 'boolean' ? value : false;
}

function readPersistedStringList(value: unknown): string[] {
  return Array.isArray(value) ? normalizeUniqueTrimmedStringsFromUnknown(value) : [];
}

function readPersistedSearchQuery(value: unknown): string {
  return typeof value === 'string' ? normalizeSearchQuery(value) : EMPTY_SEARCH_QUERY;
}

/**
 * Carries the old select-all intent forward when Xiph becomes available.
 * Explicitly empty and partial user selections remain unchanged.
 */
function migratePreXiphDefaultSelection(filters: PersistedGamesFilters): PersistedGamesFilters {
  const selectedKnownLibraries = intersectLibraries(filters.libraries, ALL_KNOWN_LIBRARIES);
  const selectedLibrarySet = new Set(selectedKnownLibraries);
  const selectedEveryPreXiphDefault =
    selectedKnownLibraries.length === PRE_XIPH_DEFAULT_LIBRARIES.length &&
    PRE_XIPH_DEFAULT_LIBRARIES.every((library) => selectedLibrarySet.has(library));

  if (!selectedEveryPreXiphDefault) {
    return filters;
  }

  return {
    ...filters,
    libraries: [...ALL_KNOWN_LIBRARIES],
  };
}
