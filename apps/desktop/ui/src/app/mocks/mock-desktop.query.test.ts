import { beforeEach, describe, expect, it } from 'vitest';
import {
  mockGetCatalogSetting,
  mockQueryGameCards,
  mockSetCatalogSetting,
  resetMockDesktopState,
} from './desktop';
import { createGameSummaryFromDetails, createManualPreviewDetails } from './desktop-state';

describe('mockQueryGameCards parity', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('returns filtered and paged results with total count', async () => {
    const baseline = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      selectedLaunchers: [],
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 100, offset: 0 },
    });

    const selectedLibrary = baseline.availableLibraries[0];
    expect(typeof selectedLibrary).toBe('string');

    const filtered = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [selectedLibrary],
      selectedLaunchers: [],
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
      selectedLaunchers: [],
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    const right = await mockQueryGameCards({
      searchQuery: 'cyber',
      selectedLibraries: ['dlss_super_resolution'],
      selectedLaunchers: [],
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    expect(left.queryFingerprint).toBe(right.queryFingerprint);
  });

  it('deletes persisted setting when value is blank', async () => {
    await mockSetCatalogSetting('games_filters_v3', '{"libraries":["x"]}');
    await mockSetCatalogSetting('games_filters_v3', '   ');
    const payload = await mockGetCatalogSetting('games_filters_v3');
    expect(payload.value).toBeNull();
  });

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
      current_version: '1.0.0',
      candidates: [
        {
          artifact_id: 'artifact:preview:unknown',
          file_name: 'mystery.dll',
          file_path: 'C:/RenderPilot/Library/mystery.dll',
          version: '2.0.0',
          source_game_id: null,
          comparison: 'newer_version',
          is_downloaded: true,
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
