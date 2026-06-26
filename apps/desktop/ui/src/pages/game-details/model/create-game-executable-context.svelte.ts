import { describeCommandErrorTechnical } from '@shared/api';
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
  // Guards a stale in-flight reload from overwriting a newer game's state.
  let activeGameId: string | null = $state(null);

  const effectiveExe = $derived(effective?.file_name ?? null);
  const effectiveExeSource = $derived(effective?.source ?? null);
  const supportedCandidates = $derived(candidates.filter((c) => c.rejection === null));
  const filteredOutCandidates = $derived(candidates.filter((c) => c.rejection !== null));

  async function reload(gameId: string): Promise<void> {
    activeGameId = gameId;
    busy = true;
    loadError = null;
    try {
      const [eff, cands] = await Promise.all([
        resolveGameExecutable(gameId),
        listGameExecutableCandidates(gameId),
      ]);
      if (activeGameId !== gameId) {
        return;
      }
      effective = eff;
      candidates = cands;
    } catch (e) {
      if (activeGameId !== gameId) {
        return;
      }
      loadError = describeCommandErrorTechnical(e);
      effective = null;
      candidates = [];
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
  }

  async function setOverride(gameId: string, absolutePath: string): Promise<void> {
    if (!gameId) {
      return;
    }
    try {
      await setGameExecutableOverride(gameId, absolutePath);
    } catch (e) {
      loadError = describeCommandErrorTechnical(e);
      return;
    }
    await reload(gameId);
    await onChange?.(gameId);
  }

  async function clearOverride(gameId: string): Promise<void> {
    if (!gameId) {
      return;
    }
    try {
      await clearGameExecutableOverride(gameId);
    } catch (e) {
      loadError = describeCommandErrorTechnical(e);
      return;
    }
    await reload(gameId);
    await onChange?.(gameId);
  }

  return {
    get busy() {
      return busy;
    },
    get loadError() {
      return loadError;
    },
    get effectiveExe() {
      return effectiveExe;
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
