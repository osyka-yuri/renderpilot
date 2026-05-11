import { beforeEach, describe, expect, it } from 'vitest';
import {
  mockGetCatalogSetting,
  mockQueryGameCards,
  mockSetCatalogSetting,
  resetMockDesktopState,
} from './desktop';

describe('mockQueryGameCards parity', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('returns filtered and paged results with total count', async () => {
    const baseline = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 100, offset: 0 },
    });

    const selectedLibrary = baseline.availableLibraries[0];
    expect(typeof selectedLibrary).toBe('string');

    const filtered = await mockQueryGameCards({
      searchQuery: '',
      selectedLibraries: [selectedLibrary],
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
      selectedLibraries: [' DlssSuperResolution ', 'DlssSuperResolution'],
      sort: { field: 'title', direction: 'asc' },
      page: { limit: 50, offset: 0 },
    });

    const right = await mockQueryGameCards({
      searchQuery: 'cyber',
      selectedLibraries: ['DlssSuperResolution'],
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
});
