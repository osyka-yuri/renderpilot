import { beforeEach, describe, expect, it } from 'vitest';
import {
  mockGetCatalogSetting,
  mockQueryGameCards,
  mockSetCatalogSetting,
  mockSetGameFavorite,
  mockSetGameHidden,
  resetMockDesktopState,
} from './desktop';
import { createGameSummaryFromDetails } from './build-game-summary';
import { createManualPreviewDetails } from './fixtures';

describe('mockQueryGameCards', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('returns filtered and paged results with total count', async () => {
    const baseline = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 100, offset: 0 },
    });

    const selectedLibrary = baseline.availableLibraries[0];
    expect(typeof selectedLibrary).toBe('string');

    const filtered = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [selectedLibrary],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 1, offset: 0 },
    });

    expect(filtered.total).toBeGreaterThanOrEqual(filtered.items.length);
    expect(filtered.items.length).toBeLessThanOrEqual(1);
    expect(filtered.items.every((item) => item.library_tags.includes(selectedLibrary))).toBe(true);
  });

  it('normalizes query fingerprint for equivalent input', async () => {
    const left = await mockQueryGameCards({
      searchQuery: '  cyber  ',
      selectedLibraries: [' dlss_super_resolution ', 'dlss_super_resolution'],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    const right = await mockQueryGameCards({
      searchQuery: 'cyber',
      selectedLibraries: ['dlss_super_resolution'],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    expect(left.queryFingerprint).toBe(right.queryFingerprint);
  });

  it('filters favoritesOnly and floats favorites when sorting', async () => {
    await mockSetGameFavorite('steam:1091500', true);

    const result = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: true,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    expect(result.total).toBe(1);
    expect(result.items[0]?.game_id).toBe('steam:1091500');
    expect(result.items[0]?.is_favorite).toBe(true);
  });

  it('hides hidden cards until showHidden is enabled', async () => {
    await mockSetGameHidden('epic:alanwake2', true);

    const hidden = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: false,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });
    expect(hidden.items.every((item) => item.game_id !== 'epic:alanwake2')).toBe(true);
    expect(hidden.hiddenCount).toBeGreaterThanOrEqual(1);

    const shown = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      selectedAddons: [],
      selectedLaunchers: [],
      showHidden: true,
      favoritesOnly: false,
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });
    expect(shown.items.some((item) => item.game_id === 'epic:alanwake2')).toBe(true);
  });
});

describe('mock catalog settings', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('deletes persisted setting when value is blank', async () => {
    await mockSetCatalogSetting('games_filters_v3', '{"libraries":["x"]}');
    await mockSetCatalogSetting('games_filters_v3', '   ');
    const payload = await mockGetCatalogSetting('games_filters_v3');
    expect(payload.value).toBeNull();
  });
});

describe('createGameSummaryFromDetails', () => {
  it('builds mock card summaries with the same visible-only library semantics as runtime', () => {
    const details = createManualPreviewDetails(
      'manual:preview:test',
      'Preview Test',
      'C:/Games/Preview Test',
    );

    details.components.push({
      id: 'manual:preview:test:unknown',
      game_id: 'manual:preview:test',
      kind: 'NativeLibrary',
      technology: 'unknown',
      swappability: 'ReadOnly',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'C:/Games/Preview Test/mystery.dll',
          version: '1.0.0',
          sha256: 'preview-unknown',
        },
      ],
    });
    details.candidate_groups.push({
      component_id: 'manual:preview:test:unknown',
      technology: 'unknown',
      file_path: 'C:/Games/Preview Test/mystery.dll',
      version_report: {
        kind: 'known',
        technical_version: '1.0.0',
        release_label: null,
        catalog_release: null,
      },
      candidates: [
        {
          artifact_id: 'artifact:preview:unknown',
          file_name: 'mystery.dll',
          file_path: 'C:/RenderPilot/Library/mystery.dll',
          technical_version: '2.0.0',
          release_label: null,
          source_game_id: null,
          comparison: 'newer_version',
          catalog_package: null,
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256',
          d3d12_executable_action: null,
        },
      ],
    });

    const summary = createGameSummaryFromDetails(details, {
      risk_level: 'medium',
      rollback_available: false,
      last_operation_status: null,
    });

    expect(summary.library_tags).toEqual(['dlss_super_resolution']);
    expect(summary.component_count).toBe(1);
    expect(summary.update_count).toBe(1);
    expect(summary.updates_available).toBe(true);
  });
});
