import { describeCommandErrorTechnical } from '@shared/api';
import { t } from '@shared/i18n';
import {
  clearGameExecutableOverride,
  createNvapiSettingsStore,
  listGameExecutableCandidates,
  listNvapiSettingStates,
  revertNvapiSetting,
  setGameExecutableOverride,
  setNvapiSettingValue,
  type ExecutableCandidate,
  type SettingStateResponse,
} from '@features/nvapi-settings';

/**
 * Single reactive context for the whole NVIDIA tab.
 *
 * Owns the live state of every DLSS catalog setting (read in one batched DRS
 * session) **and** the driver-profile executable selection. Keeping both in one
 * context matters: changing the profile executable changes which driver profile
 * every setting reads from, so an override must refresh all of them — which a
 * single `reload()` here does for free.
 *
 * The settings half is delegated to the shared {@link createNvapiSettingsStore}
 * (family grouping, warnings, optimistic writes, elevation gating); this context
 * only layers on the per-game executable resolution.
 */

export type NvidiaDriverContext = ReturnType<typeof createNvidiaDriverContext>;

export type CreateNvidiaDriverContextOptions = {
  /** Whether NVAPI writes can succeed in this process (admin). */
  isElevated: () => boolean;
};

export function createNvidiaDriverContext({ isElevated }: CreateNvidiaDriverContextOptions) {
  const store = createNvapiSettingsStore({ isElevated });

  // ── reactive state specific to per-game profile resolution ───────
  let candidates: ExecutableCandidate[] = $state([]);
  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);

  // ── derived: profile-level info (shared by all settings) ─────────
  const representative = $derived<SettingStateResponse | null>(
    store.states.length > 0 ? store.states[0] : null,
  );
  const effectiveExe = $derived(representative?.effective_exe ?? null);
  const effectiveExeSource = $derived(representative?.effective_exe_source ?? null);
  const hasProfile = $derived(representative?.has_profile_for_exe ?? false);

  const supportedCandidates = $derived(candidates.filter((c) => c.rejection === null));
  const filteredOutCandidates = $derived(candidates.filter((c) => c.rejection !== null));

  // ── actions ──────────────────────────────────────────────────────
  async function reload(gameId: string): Promise<void> {
    activeGameId = gameId;
    store.setBusy(true);
    store.setLoadError(null);
    try {
      const [stateResponse, candidatesResponse] = await Promise.all([
        listNvapiSettingStates(gameId),
        listGameExecutableCandidates(gameId),
      ]);
      if (activeGameId !== gameId) return;
      store.setStates(stateResponse);
      candidates = candidatesResponse;
    } catch (e) {
      if (activeGameId !== gameId) return;
      store.setLoadError(describeCommandErrorTechnical(e));
      store.setStates([]);
      candidates = [];
    } finally {
      if (activeGameId === gameId) store.setBusy(false);
    }
  }

  function clear(): void {
    activeGameId = null;
    candidates = [];
    store.clearAll();
  }

  async function setValue(gameId: string, key: string, wire: string): Promise<void> {
    if (!gameId || !store.ensureElevated(t('nvidia.action.changeSetting'))) return;
    await store.runWrite(key, t('nvidia.changeSettingFailed'), () =>
      setNvapiSettingValue(gameId, key, wire),
    );
  }

  async function revert(
    gameId: string,
    key: string,
    target: 'predefined' | 'baseline',
  ): Promise<void> {
    if (!gameId || !store.ensureElevated(t('nvidia.action.revertSetting'))) return;
    const label =
      target === 'predefined' ? t('nvidia.revertDefaultFailed') : t('nvidia.revertBaselineFailed');
    await store.runWrite(key, label, () => revertNvapiSetting(gameId, key, target));
  }

  async function setExecutableOverride(gameId: string, absolutePath: string): Promise<void> {
    if (!gameId) return;
    store.setBusy(true);
    try {
      await setGameExecutableOverride(gameId, absolutePath);
      await reload(gameId);
    } catch (e) {
      store.reportActionError(t('nvidia.setExeFailed'), e);
    } finally {
      store.setBusy(false);
    }
  }

  async function clearExecutableOverride(gameId: string): Promise<void> {
    if (!gameId) return;
    store.setBusy(true);
    try {
      await clearGameExecutableOverride(gameId);
      await reload(gameId);
    } catch (e) {
      store.reportActionError(t('nvidia.clearExeFailed'), e);
    } finally {
      store.setBusy(false);
    }
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
    get supportedCandidates() {
      return supportedCandidates;
    },
    get filteredOutCandidates() {
      return filteredOutCandidates;
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
    // actions
    reload,
    clear,
    setValue,
    revert,
    setExecutableOverride,
    clearExecutableOverride,
  };
}
