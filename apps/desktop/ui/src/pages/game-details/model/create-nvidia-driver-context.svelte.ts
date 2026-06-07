import { SvelteSet } from 'svelte/reactivity';
import { describeCommandErrorTechnical } from '@shared/api';
import { publishErrorNotification } from '@shared/notifications';
import { t, translateKey } from '@shared/i18n';
import {
  clearGameExecutableOverride,
  listGameExecutableCandidates,
  listNvapiSettingStates,
  revertNvapiSetting,
  setGameExecutableOverride,
  setNvapiSettingValue,
  type DllInfoDto,
  type ExecutableCandidate,
  type NvapiWarning,
  type SettingFamily,
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
 * This factory is pure `$state` + actions; the owning page (GameDetailsPage)
 * holds the `$effect` that drives `reload()` / `clear()`.
 */

export type NvidiaDriverContext = ReturnType<typeof createNvidiaDriverContext>;

export type CreateNvidiaDriverContextOptions = {
  /** Whether NVAPI writes can succeed in this process (admin). */
  isElevated: () => boolean;
};

// Substrings that classify a setting warning as session/profile-level (shown
// once on the profile card) rather than family-specific (shown on each card).
const SESSION_WARNINGS: NvapiWarning[] = [
  'noExecutable',
  'nvapiUnavailable',
  'nvapiInitFailed',
  'drsFailed',
];

function isSessionWarning(warning: NvapiWarning): boolean {
  return SESSION_WARNINGS.includes(warning);
}

function translateWarning(warning: NvapiWarning): string {
  return translateKey(`gameDetails.nvapi.warning.${warning}`, warning);
}

function distinctWarnings(values: NvapiWarning[]): NvapiWarning[] {
  return Array.from(new Set(values));
}

export function createNvidiaDriverContext({ isElevated }: CreateNvidiaDriverContextOptions) {
  // ── reactive state ───────────────────────────────────────────────
  let states: SettingStateResponse[] = $state([]);
  let candidates: ExecutableCandidate[] = $state([]);
  let loadError: string | null = $state(null);
  let busy = $state(false);
  // Keys with an in-flight write; SvelteSet is reactive on mutation.
  const pending = new SvelteSet<string>();

  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);

  // ── derived: profile-level info (shared by all settings) ─────────
  const hasStates = $derived(states.length > 0);
  const representative = $derived<SettingStateResponse | null>(
    states.length > 0 ? states[0] : null,
  );
  const effectiveExe = $derived(representative?.effective_exe ?? null);
  const effectiveExeSource = $derived(representative?.effective_exe_source ?? null);
  const hasProfile = $derived(representative?.has_profile_for_exe ?? false);
  // NVAPI (NVIDIA driver) presence — session-level, identical on every state row.
  // Optimistic default so NVIDIA users don't see a flash before the reload lands.
  const nvapiAvailable = $derived(representative?.nvapi_available ?? true);

  const supportedCandidates = $derived(candidates.filter((c) => c.rejection === null));
  const filteredOutCandidates = $derived(candidates.filter((c) => c.rejection !== null));

  const profileWarnings = $derived.by((): string[] =>
    distinctWarnings(states.flatMap((s) => s.warnings).filter(isSessionWarning)).map(
      translateWarning,
    ),
  );

  // ── per-family selectors ─────────────────────────────────────────
  function settingsForFamily(family: SettingFamily): SettingStateResponse[] {
    return states.filter((s) => s.family === family);
  }

  function familyWarnings(family: SettingFamily): string[] {
    const all = settingsForFamily(family).flatMap((s) => s.warnings);
    return distinctWarnings(all.filter((w) => !isSessionWarning(w))).map(translateWarning);
  }

  function dllInfoForFamily(family: SettingFamily): DllInfoDto | null {
    return states.find((s) => s.family === family && s.dll_info !== null)?.dll_info ?? null;
  }

  // ── actions ──────────────────────────────────────────────────────
  async function reload(gameId: string): Promise<void> {
    activeGameId = gameId;
    busy = true;
    loadError = null;
    try {
      const [stateResponse, candidatesResponse] = await Promise.all([
        listNvapiSettingStates(gameId),
        listGameExecutableCandidates(gameId),
      ]);
      if (activeGameId !== gameId) return;
      states = stateResponse;
      candidates = candidatesResponse;
    } catch (e) {
      if (activeGameId !== gameId) return;
      loadError = formatError(e);
      states = [];
      candidates = [];
    } finally {
      if (activeGameId === gameId) busy = false;
    }
  }

  function clear(): void {
    activeGameId = null;
    states = [];
    candidates = [];
    loadError = null;
    busy = false;
    pending.clear();
  }

  function patch(updated: SettingStateResponse): void {
    states = states.map((s) => (s.setting_key === updated.setting_key ? updated : s));
  }

  function reportActionError(label: string, error: unknown): void {
    publishErrorNotification(label, describeCommandErrorTechnical(error));
  }

  function ensureElevated(action: string): boolean {
    if (isElevated()) return true;
    reportActionError(t('nvidia.adminRequired'), new Error(t('nvidia.relaunchTo', { action })));
    return false;
  }

  // Runs a per-setting write, marking it pending and patching the returned
  // fresh state in place (or surfacing the error as a toast).
  async function runWrite(
    key: string,
    errorLabel: string,
    write: () => Promise<SettingStateResponse>,
  ): Promise<void> {
    pending.add(key);
    try {
      patch(await write());
    } catch (e) {
      reportActionError(errorLabel, e);
    } finally {
      pending.delete(key);
    }
  }

  async function setValue(gameId: string, key: string, wire: string): Promise<void> {
    if (!gameId || !ensureElevated(t('nvidia.action.changeSetting'))) return;
    await runWrite(key, t('nvidia.changeSettingFailed'), () =>
      setNvapiSettingValue(gameId, key, wire),
    );
  }

  async function revert(
    gameId: string,
    key: string,
    target: 'predefined' | 'baseline',
  ): Promise<void> {
    if (!gameId || !ensureElevated(t('nvidia.action.revertSetting'))) return;
    const label =
      target === 'predefined' ? t('nvidia.revertDefaultFailed') : t('nvidia.revertBaselineFailed');
    await runWrite(key, label, () => revertNvapiSetting(gameId, key, target));
  }

  async function setExecutableOverride(gameId: string, absolutePath: string): Promise<void> {
    if (!gameId) return;
    busy = true;
    try {
      await setGameExecutableOverride(gameId, absolutePath);
      await reload(gameId);
    } catch (e) {
      reportActionError(t('nvidia.setExeFailed'), e);
    } finally {
      busy = false;
    }
  }

  async function clearExecutableOverride(gameId: string): Promise<void> {
    if (!gameId) return;
    busy = true;
    try {
      await clearGameExecutableOverride(gameId);
      await reload(gameId);
    } catch (e) {
      reportActionError(t('nvidia.clearExeFailed'), e);
    } finally {
      busy = false;
    }
  }

  return {
    // state accessors
    get hasStates() {
      return hasStates;
    },
    get loadError() {
      return loadError;
    },
    get busy() {
      return busy;
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
      return nvapiAvailable;
    },
    get supportedCandidates() {
      return supportedCandidates;
    },
    get filteredOutCandidates() {
      return filteredOutCandidates;
    },
    get profileWarnings() {
      return profileWarnings;
    },
    get canWrite() {
      return isElevated();
    },
    isPending: (key: string) => pending.has(key),
    settingsForFamily,
    familyWarnings,
    dllInfoForFamily,
    // actions
    reload,
    clear,
    setValue,
    revert,
    setExecutableOverride,
    clearExecutableOverride,
  };
}

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  try {
    return JSON.stringify(err);
  } catch {
    return 'unknown error';
  }
}
