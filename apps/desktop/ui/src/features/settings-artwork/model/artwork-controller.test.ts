import { describe, expect, it, vi } from 'vitest';
import { coverSourceToggleRows, type CoverSourceToggleRow } from './artwork-model';
import { t } from '@shared/i18n';
import type { CatalogSettingPayload, CoverRemotePolicy } from '@entities/settings';
import {
  loadCoverRemoteSources,
  persistCoverSourceToggle,
  type ArtworkControllerContext,
} from './artwork-controller';
import { createInitialSettingsArtworkState } from './artwork-state';

type ArtworkGetCatalogSetting = ArtworkControllerContext['getCatalogSetting'];
type ArtworkSetCatalogSetting = ArtworkControllerContext['setCatalogSetting'];
type ArtworkFetchCoverRemotePolicy = NonNullable<
  ArtworkControllerContext['fetchCoverRemotePolicy']
>;

describe('artwork-controller', () => {
  describe('loadCoverRemoteSources', () => {
    it('loads cover source policy and finalizes busy/loaded flags', async () => {
      const fetchCoverRemotePolicy = vi.fn<ArtworkFetchCoverRemotePolicy>(() =>
        Promise.resolve({
          steamCdn: true,
          gogCdn: true,
          steamgriddb: false,
        }),
      );

      const harness = createArtworkHarness({ fetchCoverRemotePolicy });

      await loadCoverRemoteSources(harness.context);

      expect(fetchCoverRemotePolicy).toHaveBeenCalledWith(harness.context.getCatalogSetting);
      expect(harness.state.coverSourcesLoaded).toBe(true);
      expect(harness.state.coverSourcesBusy).toBe(false);
      expect(harness.state.coverSourcesState.steamCdn).toBe(true);
      expect(harness.state.coverSourcesState.gogCdn).toBe(true);
      expect(harness.state.coverSourcesState.steamgriddb).toBe(false);
      expect(harness.message).toBe('');
    });

    it('writes artwork read error and finalizes state when policy load fails', async () => {
      const harness = createArtworkHarness({
        fetchCoverRemotePolicy: vi.fn<ArtworkFetchCoverRemotePolicy>(() =>
          Promise.reject(new Error('failed')),
        ),
      });

      await loadCoverRemoteSources(harness.context);

      expect(harness.state.coverSourcesLoaded).toBe(true);
      expect(harness.state.coverSourcesBusy).toBe(false);
      expect(harness.message).toBe(t('settings.catalog.artworkReadError'));
    });

    it('ignores stale cover policy response and keeps pending state untouched', async () => {
      const harness = createArtworkHarness({
        isRequestActive: () => false,
        fetchCoverRemotePolicy: vi.fn<ArtworkFetchCoverRemotePolicy>(() =>
          Promise.resolve({
            steamCdn: true,
            gogCdn: true,
            steamgriddb: true,
          }),
        ),
      });

      await loadCoverRemoteSources(harness.context);

      expect(harness.state.coverSourcesBusy).toBe(true);
      expect(harness.state.coverSourcesLoaded).toBe(false);
      expect(harness.message).toBe('');
    });

    it('does not start cover policy load when already disposed', async () => {
      const fetchCoverRemotePolicy = vi.fn<ArtworkFetchCoverRemotePolicy>(() =>
        Promise.resolve({
          steamCdn: true,
          gogCdn: true,
          steamgriddb: true,
        }),
      );

      const harness = createArtworkHarness({
        isDisposed: () => true,
        fetchCoverRemotePolicy,
      });

      await loadCoverRemoteSources(harness.context);

      expect(fetchCoverRemotePolicy).not.toHaveBeenCalled();
      expect(harness.state).toEqual(createInitialSettingsArtworkState());
      expect(harness.message).toBe('');
    });
  });

  describe('persistCoverSourceToggle', () => {
    it('persists optimistic toggle update and clears saving flag on success', async () => {
      const row = getFirstCoverSourceToggleRow();
      const harness = createArtworkHarness({
        setCatalogSetting: vi.fn<ArtworkSetCatalogSetting>(() => Promise.resolve({ saved: true })),
      });

      harness.writeState({
        ...harness.state,
        coverSourcesLoaded: true,
      });

      const previousEnabled = harness.state.coverSourcesState[row.policyKey];

      await persistCoverSourceToggle(harness.context, row, !previousEnabled, previousEnabled);

      expect(harness.context.setCatalogSetting).toHaveBeenCalledWith(
        row.settingKey,
        String(!previousEnabled),
      );
      expect(harness.state.coverSourcesState[row.policyKey]).toBe(!previousEnabled);
      expect(harness.state.savingCoverSourceKeys.has(row.settingKey)).toBe(false);
      expect(harness.message).toBe('');
    });

    it('rolls back optimistic toggle update when save fails', async () => {
      const row = getFirstCoverSourceToggleRow();
      const harness = createArtworkHarness({
        setCatalogSetting: vi.fn<ArtworkSetCatalogSetting>(() =>
          Promise.reject(new Error('failed')),
        ),
      });

      harness.writeState({
        ...harness.state,
        coverSourcesLoaded: true,
      });

      const previousEnabled = harness.state.coverSourcesState[row.policyKey];

      await persistCoverSourceToggle(harness.context, row, !previousEnabled, previousEnabled);

      expect(harness.state.coverSourcesState[row.policyKey]).toBe(previousEnabled);
      expect(harness.state.savingCoverSourceKeys.has(row.settingKey)).toBe(false);
      expect(harness.message).toBe(t('settings.catalog.artworkSaveError'));
    });

    it('ignores stale toggle error when a newer mutation exists', async () => {
      const row = getFirstCoverSourceToggleRow();

      const harness = createArtworkHarness({
        setCatalogSetting: vi.fn<ArtworkSetCatalogSetting>(() => {
          harness.writeState({
            ...harness.state,
            coverSourceMutationVersion: {
              ...harness.state.coverSourceMutationVersion,
              [row.settingKey]: 100,
            },
          });

          return Promise.reject(new Error('failed'));
        }),
      });

      harness.writeState({
        ...harness.state,
        coverSourcesLoaded: true,
      });

      const previousEnabled = harness.state.coverSourcesState[row.policyKey];

      await persistCoverSourceToggle(harness.context, row, !previousEnabled, previousEnabled);

      expect(harness.state.coverSourcesState[row.policyKey]).toBe(!previousEnabled);
      expect(harness.state.savingCoverSourceKeys.has(row.settingKey)).toBe(true);
      expect(harness.message).toBe('');
    });

    it('does not start toggle persistence when already disposed', async () => {
      const row = getFirstCoverSourceToggleRow();

      const setCatalogSetting = vi.fn<ArtworkSetCatalogSetting>(() =>
        Promise.resolve({ saved: true }),
      );

      const harness = createArtworkHarness({
        isDisposed: () => true,
        setCatalogSetting,
      });

      const initialState = harness.state;
      const previousEnabled = initialState.coverSourcesState[row.policyKey];

      await persistCoverSourceToggle(harness.context, row, !previousEnabled, previousEnabled);

      expect(setCatalogSetting).not.toHaveBeenCalled();
      expect(harness.state).toBe(initialState);
      expect(harness.message).toBe('');
    });
  });
});

function createArtworkHarness(options?: {
  isDisposed?: () => boolean;
  isRequestActive?: (id: number) => boolean;
  getCatalogSetting?: ArtworkGetCatalogSetting;
  setCatalogSetting?: ArtworkSetCatalogSetting;
  fetchCoverRemotePolicy?: ArtworkFetchCoverRemotePolicy;
}) {
  const request = createRequestHarness({
    isDisposed: options?.isDisposed,
    isRequestActive: options?.isRequestActive,
  });

  const state = {
    artwork: createInitialSettingsArtworkState(),
    message: '',
  };

  const context: ArtworkControllerContext = {
    request: request.channel,
    getCatalogSetting:
      options?.getCatalogSetting ??
      vi.fn<ArtworkGetCatalogSetting>((key) => Promise.resolve(createCatalogPayload(key, null))),
    setCatalogSetting:
      options?.setCatalogSetting ??
      vi.fn<ArtworkSetCatalogSetting>(() => Promise.reject(new Error('failed'))),
    fetchCoverRemotePolicy: options?.fetchCoverRemotePolicy ?? createDefaultPolicyLoader(),
    state: {
      readState: () => state.artwork,
      writeState: (nextState) => {
        state.artwork = nextState;
      },
      setMessage: (value) => {
        state.message = value;
      },
    },
  };

  return {
    context,
    request,

    get state() {
      return state.artwork;
    },

    get message() {
      return state.message;
    },

    writeState(nextState: typeof state.artwork) {
      state.artwork = nextState;
    },
  };
}

function createRequestHarness(options?: {
  isDisposed?: () => boolean;
  isRequestActive?: (id: number) => boolean;
}) {
  let requestId = 0;

  const channel = {
    begin: () => {
      requestId += 1;
      return requestId;
    },
    isActive: (id: number) => {
      return options?.isRequestActive?.(id) ?? id === requestId;
    },
    isDisposed: () => {
      return options?.isDisposed?.() ?? false;
    },
    invalidate: () => {
      requestId += 1;
    },
  };

  return {
    channel,

    get requestId() {
      return requestId;
    },
  };
}

function createDefaultPolicyLoader(): ArtworkFetchCoverRemotePolicy {
  return vi.fn<ArtworkFetchCoverRemotePolicy>(() => Promise.resolve(createCoverRemotePolicy()));
}

function createCoverRemotePolicy(overrides?: Partial<CoverRemotePolicy>): CoverRemotePolicy {
  return {
    steamCdn: true,
    gogCdn: true,
    steamgriddb: true,
    ...overrides,
  };
}

function createCatalogPayload(_key: string, value: string | null): CatalogSettingPayload {
  return {
    value,
  };
}

function getFirstCoverSourceToggleRow(): CoverSourceToggleRow {
  const row = coverSourceToggleRows[0];

  return row;
}
