import { formatPresentedError } from '@shared/error-presentation';
import { reportClientError } from '@shared/errors';
import { t } from '@shared/i18n';
import { publishWarningNotification } from '@shared/notifications';
import {
  clearGameExecutableOverride,
  listGameExecutableCandidates,
  resolveGameExecutable,
  setGameExecutableOverride,
  type EffectiveExecutable,
  type ExecutableCandidate,
} from '@features/nvapi-settings';

/**
 * Game-level executable selection, shared by the NVIDIA profile target and the
 * RenoDX install location. Independent of NVAPI hardware, so it resolves for any
 * GPU and is rendered above the per-vendor tabs.
 *
 * Changing the override re-resolves the effective executable and then fires
 * {@link CreateGameExecutableContextOptions.onChange} so dependents (the NVIDIA
 * driver context, whose settings read from the profile of this exe) can refresh.
 */

export type GameExecutableContext = ReturnType<typeof createGameExecutableContext>;

export type CreateGameExecutableContextOptions = {
  /** Run after the override changes (set or cleared), with the affected game id. */
  onChange?: (gameId: string) => void | Promise<void>;
};

export function createGameExecutableContext({ onChange }: CreateGameExecutableContextOptions = {}) {
  let candidates = $state<ExecutableCandidate[]>([]);
  let effective = $state<EffectiveExecutable | null>(null);
  let busy = $state(false);
  let loadError: string | null = $state(null);
  let changeError: string | null = $state(null);
  let refreshError: string | null = $state(null);
  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);

  const effectiveExe = $derived(effective?.file_name ?? null);
  const effectiveAbsolutePath = $derived(effective?.absolute_path ?? null);
  const autoAbsolutePath = $derived(effective?.auto_absolute_path ?? null);
  const effectiveExeSource = $derived(effective?.source ?? null);
  const supportedCandidates = $derived(candidates.filter((c) => c.rejection === null));
  const filteredOutCandidates = $derived(candidates.filter((c) => c.rejection !== null));

  async function reload(gameId: string): Promise<boolean> {
    activeGameId = gameId;
    busy = true;
    loadError = null;
    refreshError = null;
    try {
      const [eff, cands] = await Promise.all([
        resolveGameExecutable(gameId),
        listGameExecutableCandidates(gameId),
      ]);
      if (activeGameId !== gameId) {
        return false;
      }
      effective = eff;
      candidates = cands;
      return true;
    } catch (e) {
      if (activeGameId !== gameId) {
        return false;
      }
      loadError = formatPresentedError(e);
      effective = null;
      candidates = [];
      return false;
    } finally {
      if (activeGameId === gameId) {
        busy = false;
      }
    }
  }

  function clear(): void {
    activeGameId = null;
    candidates = [];
    effective = null;
    loadError = null;
    changeError = null;
    refreshError = null;
  }

  async function refreshAfterCommittedChange(gameId: string): Promise<void> {
    const errors: string[] = [];
    const reloaded = await reload(gameId);
    if (!reloaded && activeGameId === gameId && loadError) {
      errors.push(loadError);
      loadError = null;
    }
    try {
      await onChange?.(gameId);
    } catch (error) {
      errors.push(formatPresentedError(error));
      reportClientError('game_executable_refresh', error);
    }
    if (errors.length > 0) {
      refreshError = errors.join('\n');
      publishWarningNotification(t('gameDetails.executable.refreshFailed'), refreshError);
    }
  }

  async function setOverride(gameId: string, absolutePath: string): Promise<boolean> {
    if (!gameId) {
      return false;
    }
    loadError = null;
    changeError = null;
    refreshError = null;
    try {
      await setGameExecutableOverride(gameId, absolutePath);
    } catch (e) {
      changeError = formatPresentedError(e);
      return false;
    }
    await refreshAfterCommittedChange(gameId);
    return true;
  }

  async function clearOverride(gameId: string): Promise<boolean> {
    if (!gameId) {
      return false;
    }
    loadError = null;
    changeError = null;
    refreshError = null;
    try {
      await clearGameExecutableOverride(gameId);
    } catch (e) {
      changeError = formatPresentedError(e);
      return false;
    }
    await refreshAfterCommittedChange(gameId);
    return true;
  }

  return {
    get busy() {
      return busy;
    },
    get loadError() {
      return loadError;
    },
    get changeError() {
      return changeError;
    },
    get refreshError() {
      return refreshError;
    },
    get effectiveExe() {
      return effectiveExe;
    },
    get effectiveAbsolutePath() {
      return effectiveAbsolutePath;
    },
    get autoAbsolutePath() {
      return autoAbsolutePath;
    },
    get effectiveExeSource() {
      return effectiveExeSource;
    },
    get supportedCandidates() {
      return supportedCandidates;
    },
    get filteredOutCandidates() {
      return filteredOutCandidates;
    },
    reload,
    clear,
    setOverride,
    clearOverride,
  };
}
