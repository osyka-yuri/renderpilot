import { formatPresentedError } from '@shared/error-presentation';
import { t } from '@shared/i18n';
import { reportClientError } from '@shared/errors';
import { publishWarningNotification } from '@shared/notifications';
import { createExclusiveTaskRunner, type ExclusiveTaskRunner } from '@shared/concurrency';
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
 * (family grouping, warnings, and confirmed setting updates).
 */

export type NvidiaDriverContext = ReturnType<typeof createNvidiaDriverContext>;

export function createNvidiaDriverContext(
  onSettingsChanged?: (gameId: string) => void | Promise<void>,
  canMutate: () => boolean = () => true,
  mutationRunner: ExclusiveTaskRunner = createExclusiveTaskRunner(),
) {
  const store = createNvapiSettingsStore();

  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);
  let activeStoreSession = 0;
  let loaded = $state(false);

  function isCurrent(gameId: string, storeSession: number): boolean {
    return activeGameId === gameId && activeStoreSession === storeSession;
  }

  // ── derived: profile-level info (shared by all settings) ─────────
  const representative = $derived<SettingStateResponse | null>(
    store.states.length > 0 ? store.states[0] : null,
  );
  const effectiveExe = $derived(representative?.effective_exe ?? null);
  const effectiveExeSource = $derived(representative?.effective_exe_source ?? null);
  const hasProfile = $derived(representative?.has_profile_for_exe ?? false);

  // ── actions ──────────────────────────────────────────────────────
  async function reload(
    gameId: string,
    options: { reportRefreshError?: boolean } = {},
  ): Promise<void> {
    activeGameId = gameId;
    const storeSession = store.beginSession();
    activeStoreSession = storeSession;
    loaded = false;
    store.setBusy(true);
    store.setLoadError(null);
    try {
      const stateResponse = await listNvapiSettingStates(gameId);
      if (!isCurrent(gameId, storeSession)) {
        return;
      }
      store.setStates(stateResponse);
    } catch (e) {
      if (!isCurrent(gameId, storeSession)) {
        return;
      }
      store.setLoadError(formatPresentedError(e));
      store.setStates([]);
      if (options.reportRefreshError) {
        reportClientError('nvapi_settings_refresh', e);
        publishWarningNotification(t('nvidia.changeSettingFailed'), formatPresentedError(e));
      }
    } finally {
      if (isCurrent(gameId, storeSession)) {
        loaded = true;
        store.setBusy(false);
      }
    }
  }

  function clear(): void {
    activeGameId = null;
    activeStoreSession = store.clearAll();
    loaded = false;
  }

  async function refreshProfileStatus(
    gameId: string,
    settingApplied: boolean,
    stillCurrent: () => boolean,
  ): Promise<void> {
    try {
      await onSettingsChanged?.(gameId);
    } catch (error) {
      if (!stillCurrent()) {
        return;
      }
      reportClientError('nvapi_profile_refresh', error);
      publishWarningNotification(
        settingApplied ? t('gameDetails.profile.refreshFailed') : t('nvidia.changeSettingFailed'),
        formatPresentedError(error),
      );
    }
  }

  async function refreshActiveGame(gameId: string): Promise<void> {
    if (activeGameId !== gameId) {
      return;
    }
    await reload(gameId, { reportRefreshError: true });
    if (activeGameId === gameId) {
      const refreshSession = activeStoreSession;
      await refreshProfileStatus(gameId, false, () => isCurrent(gameId, refreshSession));
    }
  }

  async function runSettingOperation(
    gameId: string,
    key: string,
    errorLabel: string,
    write: () => Promise<SettingStateResponse>,
  ): Promise<void> {
    if (!gameId || activeGameId !== gameId || store.busy || mutationRunner.busy || !canMutate()) {
      return;
    }

    const storeSession = activeStoreSession;
    const current = () => isCurrent(gameId, storeSession);
    await mutationRunner.run(async () => {
      const applied = await store.runWrite(key, errorLabel, write);
      if (!applied || !current()) {
        await refreshActiveGame(gameId);
      } else {
        await refreshProfileStatus(gameId, true, current);
      }
    });
  }

  async function setValue(gameId: string, key: string, wire: string): Promise<void> {
    await runSettingOperation(gameId, key, t('nvidia.changeSettingFailed'), () =>
      setNvapiSettingValue(gameId, key, wire),
    );
  }

  async function revert(
    gameId: string,
    key: string,
    target: 'predefined' | 'original',
  ): Promise<void> {
    const label =
      target === 'predefined' ? t('nvidia.revertDefaultFailed') : t('nvidia.revertOriginalFailed');
    await runSettingOperation(gameId, key, label, () => revertNvapiSetting(gameId, key, target));
  }

  return {
    // state accessors (settings half delegated to the shared store)
    get hasStates() {
      return store.hasStates;
    },
    get loaded() {
      return loaded;
    },
    get loadError() {
      return store.loadError;
    },
    get busy() {
      return store.busy || mutationRunner.busy;
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
