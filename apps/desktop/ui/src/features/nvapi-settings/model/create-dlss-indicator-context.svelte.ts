import { formatPresentedError } from '@shared/error-presentation';
import { reportClientError } from '@shared/errors';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { getDlssIndicatorState, setDlssIndicatorEnabled } from '../api/desktop';

/**
 * Reactive owner of the system-wide NVIDIA DLSS indicator overlay toggle.
 *
 * Unlike the per-game NVIDIA driver context this is **global, not per-game**: the
 * indicator is a single machine-wide registry value the NGX runtime reads for
 * every DLSS title, so there is no `gameId` and it is loaded once when the
 * Settings → NVIDIA tab is first shown. `setEnabled` reverts its optimistic
 * flip if the backend rejects the write.
 */

export type DlssIndicatorContext = ReturnType<typeof createDlssIndicatorContext>;

export function createDlssIndicatorContext() {
  // ── reactive state ───────────────────────────────────────────────
  let enabled = $state(false);
  let supported = $state(true);
  let loaded = $state(false);
  let busy = $state(false);
  let error: string | null = $state(null);

  // Plain (non-reactive) re-entrancy guard for the one-shot load.
  let inFlight = false;

  function reportActionError(label: string, e: unknown): void {
    reportClientError('dlss_indicator_action', e);
    publishPresentedErrorNotification(label, e);
  }

  // ── actions ──────────────────────────────────────────────────────
  async function load(): Promise<void> {
    if (inFlight) {
      return;
    }
    inFlight = true;
    busy = true;
    error = null;
    try {
      const state = await getDlssIndicatorState();
      enabled = state.enabled;
      supported = state.supported;
    } catch (e) {
      error = formatPresentedError(e);
    } finally {
      loaded = true;
      busy = false;
      inFlight = false;
    }
  }

  async function setEnabled(next: boolean): Promise<void> {
    if (busy || next === enabled) {
      return;
    }
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
    // actions
    load,
    setEnabled,
  };
}
