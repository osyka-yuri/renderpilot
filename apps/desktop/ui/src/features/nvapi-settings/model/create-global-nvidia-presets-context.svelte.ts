import { t } from '@shared/i18n';
import { formatPresentedError } from '@shared/error-presentation';
import { reportClientError } from '@shared/errors';
import { publishWarningNotification } from '@shared/notifications';
import { createExclusiveTaskRunner, type ExclusiveTaskRunner } from '@shared/concurrency';
import {
  listGlobalNvapiSettingStates,
  revertGlobalNvapiSetting,
  setGlobalNvapiSettingValue,
} from '../api/desktop';
import {
  createNvapiSettingsStore,
  type NvapiSettingsStore,
} from './create-nvapi-settings-store.svelte';
import type { SettingStateResponse } from './types';

/**
 * Reactive owner of NVIDIA's **global/base** DLSS driver settings.
 *
 * Unlike the per-game NVIDIA driver context this targets the base profile
 * (`_GLOBAL_DRIVER_PROFILE_`), so there is no executable selection or
 * per-game original state to restore. It composes the shared {@link createNvapiSettingsStore}
 * for family grouping, warnings, and confirmed setting updates. The first read
 * starts when the Settings → NVIDIA tab is shown.
 */

export type GlobalNvidiaPresetsContext = ReturnType<typeof createGlobalNvidiaPresetsContext>;

export function createGlobalNvidiaPresetsContext(
  mutationRunner: ExclusiveTaskRunner = createExclusiveTaskRunner(),
) {
  const store: NvapiSettingsStore = createNvapiSettingsStore();

  let loaded = $state(false);

  async function read(afterFailedWrite = false): Promise<void> {
    if (store.busy || (mutationRunner.busy && !afterFailedWrite)) {
      return;
    }
    store.setBusy(true);
    store.setLoadError(null);
    try {
      store.setStates(await listGlobalNvapiSettingStates());
    } catch (e) {
      const message = formatPresentedError(e);
      store.setLoadError(message);
      store.setStates([]);
      reportClientError('global_nvapi_settings_load', e);
      if (afterFailedWrite) {
        publishWarningNotification(t('nvidia.changeSettingFailed'), message);
      }
    } finally {
      loaded = true;
      store.setBusy(false);
    }
  }

  async function load(): Promise<void> {
    if (!loaded) {
      await read();
    }
  }

  async function refresh(): Promise<void> {
    await read();
  }

  async function runSettingOperation(
    key: string,
    errorLabel: string,
    write: () => Promise<SettingStateResponse>,
  ): Promise<void> {
    if (store.busy || mutationRunner.busy) {
      return;
    }
    await mutationRunner.run(async () => {
      if (!(await store.runWrite(key, errorLabel, write))) {
        await read(true);
      }
    });
  }

  async function setValue(key: string, wire: string): Promise<void> {
    await runSettingOperation(key, t('nvidia.changeSettingFailed'), () =>
      setGlobalNvapiSettingValue(key, wire),
    );
  }

  async function revertDefault(key: string): Promise<void> {
    await runSettingOperation(key, t('nvidia.revertDefaultFailed'), () =>
      revertGlobalNvapiSetting(key, 'predefined'),
    );
  }

  return {
    get loaded() {
      return loaded;
    },
    get hasStates() {
      return store.hasStates;
    },
    get loadError() {
      return store.loadError;
    },
    get busy() {
      return store.busy || mutationRunner.busy;
    },
    get nvapiAvailable() {
      return store.nvapiAvailable;
    },
    get profileWarnings() {
      return store.profileWarnings;
    },
    isPending: (key: string) => store.isPending(key),
    settingsForFamily: store.settingsForFamily,
    familyWarnings: store.familyWarnings,
    dllInfoForFamily: store.dllInfoForFamily,
    load,
    refresh,
    setValue,
    revertDefault,
  };
}
