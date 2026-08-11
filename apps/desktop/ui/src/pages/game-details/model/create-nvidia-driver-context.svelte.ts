import { formatPresentedError } from '@shared/error-presentation';
import { t } from '@shared/i18n';
import {
  createNvapiSettingsStore,
  listNvapiSettingStates,
  revertNvapiSetting,
  setNvapiSettingValue,
  type SettingStateResponse,
} from '@features/nvapi-settings';

/**
 * Reactive context for the NVIDIA tab's driver settings.
 *
 * Owns the live state of every DLSS catalog setting (read in one batched DRS
 * session) and the profile status (whether the effective exe has a driver
 * profile). The **executable selection** itself lives in the shared game-level
 * {@link createGameExecutableContext}; when the user changes it there, the page
 * calls {@link reload} so every setting re-reads from the new profile's exe.
 *
 * The settings half is delegated to the shared {@link createNvapiSettingsStore}
 * (family grouping, warnings, optimistic writes).
 */

export type NvidiaDriverContext = ReturnType<typeof createNvidiaDriverContext>;

export function createNvidiaDriverContext() {
  const store = createNvapiSettingsStore();

  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);

  // ── derived: profile-level info (shared by all settings) ─────────
  const representative = $derived<SettingStateResponse | null>(
    store.states.length > 0 ? store.states[0] : null,
  );
  const effectiveExe = $derived(representative?.effective_exe ?? null);
  const effectiveExeSource = $derived(representative?.effective_exe_source ?? null);
  const hasProfile = $derived(representative?.has_profile_for_exe ?? false);

  // ── actions ──────────────────────────────────────────────────────
  async function reload(gameId: string): Promise<void> {
    activeGameId = gameId;
    store.setBusy(true);
    store.setLoadError(null);
    try {
      const stateResponse = await listNvapiSettingStates(gameId);
      if (activeGameId !== gameId) {
        return;
      }
      store.setStates(stateResponse);
    } catch (e) {
      if (activeGameId !== gameId) {
        return;
      }
      store.setLoadError(formatPresentedError(e));
      store.setStates([]);
    } finally {
      if (activeGameId === gameId) {
        store.setBusy(false);
      }
    }
  }

  function clear(): void {
    activeGameId = null;
    store.clearAll();
  }

  async function setValue(gameId: string, key: string, wire: string): Promise<void> {
    if (!gameId) {
      return;
    }
    await store.runWrite(key, t('nvidia.changeSettingFailed'), () =>
      setNvapiSettingValue(gameId, key, wire),
    );
  }

  async function revert(
    gameId: string,
    key: string,
    target: 'predefined' | 'baseline',
  ): Promise<void> {
    if (!gameId) {
      return;
    }
    const label =
      target === 'predefined' ? t('nvidia.revertDefaultFailed') : t('nvidia.revertBaselineFailed');
    await store.runWrite(key, label, () => revertNvapiSetting(gameId, key, target));
  }

  return {
    // state accessors (settings half delegated to the shared store)
    get hasStates() {
      return store.hasStates;
    },
    get loadError() {
      return store.loadError;
    },
    get busy() {
      return store.busy;
    },
    get effectiveExe() {
      return effectiveExe;
    },
    get effectiveExeSource() {
      return effectiveExeSource;
    },
    get hasProfile() {
      return hasProfile;
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
    // actions
    reload,
    clear,
    setValue,
    revert,
  };
}
