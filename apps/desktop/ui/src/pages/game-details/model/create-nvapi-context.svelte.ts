import { describeCommandErrorTechnical } from '@shared/api';
import { publishErrorNotification } from '@shared/notifications';
import {
  clearGameExecutableOverride,
  getNvapiSettingState,
  listGameExecutableCandidates,
  revertNvapiSetting,
  setGameExecutableOverride,
  setNvapiSettingValue,
  type ExecutableCandidate,
  type SettingStateResponse,
  type ValueOption,
} from '@features/nvapi-settings';

/**
 * Shared NVAPI / DLSS reactive context for the GameDetailsPage.
 *
 * Owned by GameDetailsPage and passed by props into NvidiaProfileCard
 * and DlssSrComponentCard. The factory itself is pure `$state` + actions —
 * no `$effect` inside. The owning component holds the `$effect` that calls
 * `reload()` / `clear()` based on the page's `gameId` and `hasNvidiaTab`.
 *
 * This split (factory = state + actions, page = lifecycle) follows the
 * proven pattern from `create-game-workspace-model.svelte.ts` and avoids
 * the orphan-effect lifecycle bugs we hit when a previous iteration put
 * `$effect` inside an external factory function.
 */

export const DLSS_SR_SETTING_KEY = 'dlss_sr_render_preset';

export type NvapiContext = ReturnType<typeof createNvapiContext>;

export type CreateNvapiContextOptions = {
  /**
   * Whether NVAPI writes can succeed in this process. Session-stable,
   * sourced from the app initialization snapshot. When `false`, the UI
   * disables preset/revert controls and the factory's action methods
   * short-circuit with an error toast before touching the IPC layer.
   */
  isElevated: () => boolean;
};

export function createNvapiContext({ isElevated }: CreateNvapiContextOptions) {
  // ── reactive state ───────────────────────────────────────────────
  let snapshot: SettingStateResponse | null = $state(null);
  let candidates: ExecutableCandidate[] = $state([]);
  let loadError: string | null = $state(null);
  let busy = $state(false);

  // The id we last loaded for; used so that an in-flight `reload()`
  // resolving for an old game doesn't overwrite the new game's state.
  let activeGameId: string | null = $state(null);

  // ── derived getters ──────────────────────────────────────────────
  const hasSnapshot = $derived(snapshot !== null);
  const supportedCandidates = $derived(candidates.filter((c) => c.rejection === null));
  const filteredOutCandidates = $derived(candidates.filter((c) => c.rejection !== null));
  const orderedValues = $derived.by((): ValueOption[] => {
    if (!snapshot) return [];
    return [...snapshot.available_values].sort((a, b) => {
      if (a.supported !== b.supported) return a.supported ? -1 : 1;
      return 0;
    });
  });

  // ── actions ──────────────────────────────────────────────────────
  async function reload(gameId: string): Promise<void> {
    activeGameId = gameId;
    busy = true;
    loadError = null;
    try {
      const [stateResponse, candidatesResponse] = await Promise.all([
        getNvapiSettingState(gameId, DLSS_SR_SETTING_KEY),
        listGameExecutableCandidates(gameId),
      ]);
      // Discard the result if the active game changed mid-flight.
      if (activeGameId !== gameId) return;
      snapshot = stateResponse;
      candidates = candidatesResponse;
    } catch (e) {
      if (activeGameId !== gameId) return;
      loadError = formatError(e);
      snapshot = null;
      candidates = [];
    } finally {
      if (activeGameId === gameId) busy = false;
    }
  }

  function clear(): void {
    activeGameId = null;
    snapshot = null;
    candidates = [];
    loadError = null;
    busy = false;
  }

  /**
   * Surface action errors (set / revert / override) as toast notifications
   * rather than the inline `loadError` banner. The banner is reserved for
   * read-time problems (failed initial reload). Action errors are transient
   * — a toast is the right affordance, and it shows up at the page level
   * instead of next to a card that the failure didn't actually originate
   * from.
   */
  function reportActionError(actionLabel: string, error: unknown): void {
    publishErrorNotification(actionLabel, describeCommandErrorTechnical(error));
  }

  async function setPresetValue(gameId: string, wire: string): Promise<void> {
    if (!gameId) return;
    // Defense-in-depth: the UI also disables the control when !isElevated,
    // but a stray invocation (DevTools, keyboard shortcut, etc.) shouldn't
    // round-trip to NVAPI just to come back with a privilege error.
    if (!isElevated()) {
      reportActionError(
        'Administrator privileges required',
        new Error('Relaunch RenderPilot as administrator to change the DLSS preset.'),
      );
      return;
    }
    busy = true;
    try {
      snapshot = await setNvapiSettingValue(gameId, DLSS_SR_SETTING_KEY, wire);
    } catch (e) {
      reportActionError('Could not change DLSS preset', e);
    } finally {
      busy = false;
    }
  }

  async function revertPreset(gameId: string, target: 'predefined' | 'baseline'): Promise<void> {
    if (!gameId) return;
    if (!isElevated()) {
      reportActionError(
        'Administrator privileges required',
        new Error('Relaunch RenderPilot as administrator to revert this NVIDIA setting.'),
      );
      return;
    }
    busy = true;
    try {
      snapshot = await revertNvapiSetting(gameId, DLSS_SR_SETTING_KEY, target);
    } catch (e) {
      const label =
        target === 'predefined'
          ? 'Could not revert to driver default'
          : 'Could not revert to baseline';
      reportActionError(label, e);
    } finally {
      busy = false;
    }
  }

  async function setExecutableOverride(gameId: string, absolutePath: string): Promise<void> {
    if (!gameId) return;
    busy = true;
    try {
      await setGameExecutableOverride(gameId, absolutePath);
      await reload(gameId);
    } catch (e) {
      reportActionError('Could not set executable override', e);
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
      reportActionError('Could not clear executable override', e);
    } finally {
      busy = false;
    }
  }

  return {
    // ── state accessors ────────────────────────────────────────────
    get snapshot() {
      return snapshot;
    },
    get candidates() {
      return candidates;
    },
    get loadError() {
      return loadError;
    },
    get busy() {
      return busy;
    },
    get hasSnapshot() {
      return hasSnapshot;
    },
    get supportedCandidates() {
      return supportedCandidates;
    },
    get filteredOutCandidates() {
      return filteredOutCandidates;
    },
    get orderedValues() {
      return orderedValues;
    },
    get dllInfo() {
      return snapshot?.dll_info ?? null;
    },
    get baseline() {
      return snapshot?.baseline ?? null;
    },
    get warnings() {
      return snapshot?.warnings ?? [];
    },
    get effectiveExe() {
      return snapshot?.effective_exe ?? null;
    },
    get effectiveExeSource() {
      return snapshot?.effective_exe_source ?? null;
    },
    get hasProfile() {
      return snapshot?.has_profile_for_exe ?? false;
    },
    get isModifiedOutside() {
      return snapshot?.is_modified_outside_renderpilot ?? false;
    },
    /**
     * Whether NVAPI is actually available for this game. False when the
     * driver is missing, the user is on a non-NVIDIA system, or NVAPI
     * loading failed for some other reason — surfaced via `loadError`.
     */
    get nvapiAvailable() {
      return loadError === null && snapshot !== null;
    },
    /**
     * Whether NVAPI writes (set preset / revert) can succeed in this
     * session. UI controls bind their disabled state to this getter.
     */
    get canWrite() {
      return isElevated();
    },
    // ── actions ────────────────────────────────────────────────────
    reload,
    clear,
    setPresetValue,
    revertPreset,
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
