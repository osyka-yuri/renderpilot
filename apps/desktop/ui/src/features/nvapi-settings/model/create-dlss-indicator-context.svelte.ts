import { describeCommandErrorTechnical } from '@shared/api';
import { publishErrorNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { getDlssIndicatorState, setDlssIndicatorEnabled } from '../api/desktop';

/**
 * Reactive owner of the system-wide NVIDIA DLSS indicator overlay toggle.
 *
 * Unlike the per-game NVIDIA driver context this is **global, not per-game**: the
 * indicator is a single machine-wide registry value the NGX runtime reads for
 * every DLSS title, so there is no `gameId` and it is loaded once when the
 * Settings → NVIDIA tab is first shown. Reading the value works unprivileged;
 * writing it needs an elevated process, so `setEnabled` is gated on `isElevated`
 * and reverts its optimistic flip if the backend rejects the write.
 */

export type DlssIndicatorContext = ReturnType<typeof createDlssIndicatorContext>;

export type CreateDlssIndicatorContextOptions = {
  /** Whether registry writes can succeed in this process (admin). */
  isElevated: () => boolean;
};

export function createDlssIndicatorContext({ isElevated }: CreateDlssIndicatorContextOptions) {
  // ── reactive state ───────────────────────────────────────────────
  let enabled = $state(false);
  let supported = $state(true);
  let loaded = $state(false);
  let busy = $state(false);
  let error: string | null = $state(null);

  // Plain (non-reactive) re-entrancy guard for the one-shot load.
  let inFlight = false;

  function reportActionError(label: string, e: unknown): void {
    publishErrorNotification(label, describeCommandErrorTechnical(e));
  }

  // ── actions ──────────────────────────────────────────────────────
  async function load(): Promise<void> {
    if (inFlight) return;
    inFlight = true;
    busy = true;
    error = null;
    try {
      const state = await getDlssIndicatorState();
      enabled = state.enabled;
      supported = state.supported;
    } catch (e) {
      error = describeCommandErrorTechnical(e);
    } finally {
      loaded = true;
      busy = false;
      inFlight = false;
    }
  }

  function ensureElevated(): boolean {
    if (isElevated()) return true;
    reportActionError(t('nvidia.adminRequired'), new Error(t('indicator.relaunchToToggle')));
    return false;
  }

  async function setEnabled(next: boolean): Promise<void> {
    if (busy || next === enabled) return;
    if (!ensureElevated()) return;

    const previous = enabled;
    // Optimistic: reflect the new state immediately, revert if the write fails.
    enabled = next;
    busy = true;
    try {
      const state = await setDlssIndicatorEnabled(next);
      enabled = state.enabled;
      supported = state.supported;
    } catch (e) {
      enabled = previous;
      reportActionError(t('indicator.changeFailed'), e);
    } finally {
      busy = false;
    }
  }

  return {
    // state accessors
    get enabled() {
      return enabled;
    },
    get supported() {
      return supported;
    },
    get loaded() {
      return loaded;
    },
    get busy() {
      return busy;
    },
    get error() {
      return error;
    },
    get canWrite() {
      return isElevated();
    },
    // actions
    load,
    setEnabled,
  };
}
