import { describe, expect, it, vi } from 'vitest';
import type { CoverRemotePolicy } from '@shared/covers/cover-sync';
import type { CoverSourceToggleRow } from '@features/settings/settings-screen-model';
import {
  artworkSettingsReadError,
  artworkSourceSaveError,
  catalogReadError,
  coverSourceToggleRows,
  steamGridDbSettingKey,
  steamKeySaveError,
} from '@features/settings/settings-screen-model';
import {
  loadCoverRemoteSources,
  loadSteamGridDbKey,
  persistCoverSourceToggle,
  saveSteamGridDbKey,
  type ArtworkControllerContext,
  type SteamKeyControllerContext,
} from '@features/settings/settings-screen-controller';
import { createInitialSettingsArtworkState } from '@features/settings/settings-screen-state';

type SteamGetCatalogSetting = SteamKeyControllerContext['getCatalogSetting'];
type SteamSetCatalogSetting = SteamKeyControllerContext['setCatalogSetting'];

type ArtworkGetCatalogSetting = ArtworkControllerContext['getCatalogSetting'];
type ArtworkSetCatalogSetting = ArtworkControllerContext['setCatalogSetting'];
type ArtworkFetchCoverRemotePolicy = NonNullable<
  ArtworkControllerContext['fetchCoverRemotePolicy']
>;

describe('settings-screen-controller', () => {
  describe('loadSteamGridDbKey', () => {
    it('loads steam key and finalizes state', async () => {
      const getCatalogSetting = vi.fn<SteamGetCatalogSetting>(() =>
        Promise.resolve(createCatalogPayload(steamGridDbSettingKey, 'saved-key')),
      );

      const harness = createSteamHarness({ getCatalogSetting });

      await loadSteamGridDbKey(harness.context);

      expect(getCatalogSetting).toHaveBeenCalledWith(steamGridDbSettingKey);
      expect(harness.input).toBe('saved-key');
      expect(harness.loaded).toBe(true);
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(false);
    });

    it('normalizes missing steam key value to empty string', async () => {
      const getCatalogSetting = vi.fn<SteamGetCatalogSetting>(() =>
        Promise.resolve(createCatalogPayload(steamGridDbSettingKey, null)),
      );

      const harness = createSteamHarness({
        input: 'previous-key',
        getCatalogSetting,
      });

      await loadSteamGridDbKey(harness.context);

      expect(harness.input).toBe('');
      expect(harness.loaded).toBe(true);
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(false);
    });

    it('writes read error and finalizes state when steam key load fails', async () => {
      const harness = createSteamHarness({
        getCatalogSetting: vi.fn<SteamGetCatalogSetting>(() => Promise.reject(new Error('failed'))),
      });

      await loadSteamGridDbKey(harness.context);

      expect(harness.input).toBe('');
      expect(harness.loaded).toBe(true);
      expect(harness.message).toBe(catalogReadError);
      expect(harness.busy).toBe(false);
    });

    it('ignores stale steam key load response', async () => {
      const harness = createSteamHarness({
        isRequestActive: () => false,
        getCatalogSetting: vi.fn<SteamGetCatalogSetting>(() =>
          Promise.resolve(createCatalogPayload(steamGridDbSettingKey, 'new-key')),
        ),
      });

      await loadSteamGridDbKey(harness.context);

      expect(harness.input).toBe('');
      expect(harness.loaded).toBe(false);
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(true);
    });

    it('does not start steam key load when already disposed', async () => {
      const getCatalogSetting = vi.fn<SteamGetCatalogSetting>(() =>
        Promise.resolve(createCatalogPayload(steamGridDbSettingKey, 'should-not-apply')),
      );

      const harness = createSteamHarness({
        isDisposed: () => true,
        getCatalogSetting,
      });

      await loadSteamGridDbKey(harness.context);

      expect(getCatalogSetting).not.toHaveBeenCalled();
      expect(harness.input).toBe('');
      expect(harness.loaded).toBe(false);
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(false);
    });
  });

  describe('saveSteamGridDbKey', () => {
    it('saves trimmed steam key and writes success message', async () => {
      const setCatalogSetting = vi.fn<SteamSetCatalogSetting>(() =>
        Promise.resolve({ saved: true }),
      );

      const harness = createSteamHarness({
        input: '  abc123  ',
        setCatalogSetting,
      });

      await saveSteamGridDbKey(harness.context);

      expect(setCatalogSetting).toHaveBeenCalledWith(steamGridDbSettingKey, 'abc123');
      expect(harness.input).toBe('abc123');
      expect(harness.message).toBe('Saved.');
      expect(harness.busy).toBe(false);
    });

    it('clears steam key and writes cleared message', async () => {
      const setCatalogSetting = vi.fn<SteamSetCatalogSetting>(() =>
        Promise.resolve({ saved: true }),
      );

      const harness = createSteamHarness({
        input: '   ',
        setCatalogSetting,
      });

      await saveSteamGridDbKey(harness.context);

      expect(setCatalogSetting).toHaveBeenCalledWith(steamGridDbSettingKey, '');
      expect(harness.input).toBe('');
      expect(harness.message).toBe('Key cleared.');
      expect(harness.busy).toBe(false);
    });

    it('writes save error and finalizes busy state when steam key save fails', async () => {
      const harness = createSteamHarness({
        input: 'abc123',
        setCatalogSetting: vi.fn<SteamSetCatalogSetting>(() => Promise.reject(new Error('failed'))),
      });

      await saveSteamGridDbKey(harness.context);

      expect(harness.input).toBe('abc123');
      expect(harness.message).toBe(steamKeySaveError);
      expect(harness.busy).toBe(false);
    });

    it('ignores stale steam key save response', async () => {
      const setCatalogSetting = vi.fn<SteamSetCatalogSetting>(() =>
        Promise.resolve({ saved: true }),
      );

      const harness = createSteamHarness({
        input: '  stale-value  ',
        isRequestActive: () => false,
        setCatalogSetting,
      });

      await saveSteamGridDbKey(harness.context);

      expect(setCatalogSetting).toHaveBeenCalledWith(steamGridDbSettingKey, 'stale-value');
      expect(harness.input).toBe('  stale-value  ');
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(true);
    });

    it('does not start steam key save when already disposed', async () => {
      const setCatalogSetting = vi.fn<SteamSetCatalogSetting>(() =>
        Promise.resolve({ saved: true }),
      );

      const harness = createSteamHarness({
        input: 'abc123',
        isDisposed: () => true,
        setCatalogSetting,
      });

      await saveSteamGridDbKey(harness.context);

      expect(setCatalogSetting).not.toHaveBeenCalled();
      expect(harness.input).toBe('abc123');
      expect(harness.message).toBe('');
      expect(harness.busy).toBe(false);
    });
  });

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
      expect(harness.message).toBe(artworkSettingsReadError);
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
      expect(harness.message).toBe(artworkSourceSaveError);
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

function createSteamHarness(options?: {
  input?: string;
  isDisposed?: () => boolean;
  isRequestActive?: (id: number) => boolean;
  getCatalogSetting?: SteamGetCatalogSetting;
  setCatalogSetting?: SteamSetCatalogSetting;
}) {
  const request = createRequestHarness({
    isDisposed: options?.isDisposed,
    isRequestActive: options?.isRequestActive,
  });

  const state = {
    busy: false,
    loaded: false,
    input: options?.input ?? '',
    message: '',
  };

  const context: SteamKeyControllerContext = {
    request: request.channel,
    getCatalogSetting:
      options?.getCatalogSetting ??
      vi.fn<SteamGetCatalogSetting>(() =>
        Promise.resolve(createCatalogPayload(steamGridDbSettingKey, null)),
      ),
    setCatalogSetting:
      options?.setCatalogSetting ??
      vi.fn<SteamSetCatalogSetting>(() => Promise.resolve({ saved: true })),
    state: {
      readInput: () => state.input,
      writeInput: (value) => {
        state.input = value;
      },
      setBusy: (value) => {
        state.busy = value;
      },
      setLoaded: (value) => {
        state.loaded = value;
      },
      setMessage: (value) => {
        state.message = value;
      },
    },
  };

  return {
    context,
    request,

    get busy() {
      return state.busy;
    },

    get loaded() {
      return state.loaded;
    },

    get input() {
      return state.input;
    },

    get message() {
      return state.message;
    },
  };
}

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

function createCatalogPayload(key: string, value: string | null) {
  return {
    key,
    value,
  };
}

function getFirstCoverSourceToggleRow(): CoverSourceToggleRow {
  const row = coverSourceToggleRows[0];

  return row;
}
