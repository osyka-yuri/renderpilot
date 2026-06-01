import { describe, expect, it, vi } from 'vitest';
import type { CatalogSettingPayload } from '@entities/settings';
import {
  loadSteamGridDbKey,
  saveSteamGridDbKey,
  type SteamKeyControllerContext,
} from './steam-key-controller';
import { steamGridDbSettingKey } from './steam-key-model';
import { t } from '@shared/i18n';

type SteamGetCatalogSetting = SteamKeyControllerContext['getCatalogSetting'];
type SteamSetCatalogSetting = SteamKeyControllerContext['setCatalogSetting'];

describe('steam-key-controller', () => {
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
      expect(harness.message).toBe(t('settings.catalog.steamKey.readError'));
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
      expect(harness.message).toBe(t('settings.catalog.steamKey.saved'));
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
      expect(harness.message).toBe(t('settings.catalog.steamKey.cleared'));
      expect(harness.busy).toBe(false);
    });

    it('writes save error and finalizes busy state when steam key save fails', async () => {
      const harness = createSteamHarness({
        input: 'abc123',
        setCatalogSetting: vi.fn<SteamSetCatalogSetting>(() => Promise.reject(new Error('failed'))),
      });

      await saveSteamGridDbKey(harness.context);

      expect(harness.input).toBe('abc123');
      expect(harness.message).toBe(t('settings.catalog.steamKey.saveError'));
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

function createCatalogPayload(_key: string, value: string | null): CatalogSettingPayload {
  return {
    value,
  };
}
