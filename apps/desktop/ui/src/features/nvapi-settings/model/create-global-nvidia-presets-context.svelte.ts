import { t } from '@shared/i18n';
import { describeCommandErrorTechnical } from '@shared/api';
import {
  listGlobalNvapiSettingStates,
  revertGlobalNvapiSetting,
  setGlobalNvapiSettingValue,
} from '../api/desktop';
import {
  createNvapiSettingsStore,
  type NvapiSettingsStore,
} from './create-nvapi-settings-store.svelte';

/**
 * Reactive owner of NVIDIA's **global/base** DLSS driver settings.
 *
 * Unlike the per-game NVIDIA driver context this targets the base profile
 * (`_GLOBAL_DRIVER_PROFILE_`), so there is no executable selection and no
 * per-game baseline. It composes the shared {@link createNvapiSettingsStore}
 * for everything else (family grouping, warnings, optimistic writes, elevation
 * gating) and is loaded once when the Settings → NVIDIA tab is first shown.
 */

export type GlobalNvidiaPresetsContext = ReturnType<typeof createGlobalNvidiaPresetsContext>;

export type CreateGlobalNvidiaPresetsContextOptions = {
  /** Whether NVAPI writes can succeed in this process (admin). */
  isElevated: () => boolean;
};

export function createGlobalNvidiaPresetsContext({
  isElevated,
}: CreateGlobalNvidiaPresetsContextOptions) {
  const store: NvapiSettingsStore = createNvapiSettingsStore({ isElevated });

  let loaded = $state(false);
  // Plain (non-reactive) re-entrancy guard for the one-shot load.
  let inFlight = false;

  async function load(): Promise<void> {
    if (inFlight) {
      return;
    }
    inFlight = true;
    store.setBusy(true);
    store.setLoadError(null);
    try {
      store.setStates(await listGlobalNvapiSettingStates());
    } catch (e) {
      store.setLoadError(describeCommandErrorTechnical(e));
      store.setStates([]);
    } finally {
      loaded = true;
      store.setBusy(false);
      inFlight = false;
    }
  }

  async function setValue(key: string, wire: string): Promise<void> {
    if (!store.ensureElevated(t('nvidia.action.changeSetting'))) {
      return;
    }
    await store.runWrite(key, t('nvidia.changeSettingFailed'), () =>
      setGlobalNvapiSettingValue(key, wire),
    );
  }

  async function revertDefault(key: string): Promise<void> {
    if (!store.ensureElevated(t('nvidia.action.revertSetting'))) {
      return;
    }
    await store.runWrite(key, t('nvidia.revertDefaultFailed'), () =>
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
      return store.busy;
    },
    get nvapiAvailable() {
      return store.nvapiAvailable;
    },
    get profileWarnings() {
      return store.profileWarnings;
    },
    get canWrite() {
      return store.canWrite;
    },
    isPending: (key: string) => store.isPending(key),
    settingsForFamily: store.settingsForFamily,
    familyWarnings: store.familyWarnings,
    dllInfoForFamily: store.dllInfoForFamily,
    load,
    setValue,
    revertDefault,
  };
}
