import { SvelteSet } from 'svelte/reactivity';
import { ClientError, reportClientError } from '@shared/errors';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import type { DllInfoDto, NvapiWarning, SettingFamily, SettingStateResponse } from './types';

/**
 * Shared, scope-agnostic core for any NVAPI driver-settings UI.
 *
 * Owns the live `SettingStateResponse[]`, the in-flight write set, and all of
 * the logic that does not depend on *where* the settings live (per-game profile
 * vs. the global base profile): family grouping, warning classification,
 * optimistic per-setting writes, and elevation gating. The per-game and global
 * contexts compose this store and only add what differs (executable selection
 * for per-game; nothing extra for global), so none of this logic is duplicated.
 */

// Substrings that classify a setting warning as session/profile-level (shown
// once for the whole profile) rather than family-specific (shown per family).
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
  return t(`gameDetails.nvapi.warning.${warning}`);
}

function distinctWarnings(values: NvapiWarning[]): NvapiWarning[] {
  return Array.from(new Set(values));
}

export type NvapiSettingsStore = ReturnType<typeof createNvapiSettingsStore>;

export type CreateNvapiSettingsStoreOptions = {
  /** Whether NVAPI writes can succeed in this process (admin). */
  isElevated: () => boolean;
};

export function createNvapiSettingsStore({ isElevated }: CreateNvapiSettingsStoreOptions) {
  // ── reactive state ───────────────────────────────────────────────
  let states: SettingStateResponse[] = $state([]);
  let busy = $state(false);
  let loadError: string | null = $state(null);
  // Keys with an in-flight write; SvelteSet is reactive on mutation.
  const pending = new SvelteSet<string>();

  // ── derived: profile-level info (shared by all settings) ─────────
  const hasStates = $derived(states.length > 0);
  const representative = $derived<SettingStateResponse | null>(
    states.length > 0 ? states[0] : null,
  );
  // NVAPI (NVIDIA driver) presence — session-level, identical on every row.
  // Optimistic default so NVIDIA users don't see a flash before the load lands.
  const nvapiAvailable = $derived(representative?.nvapi_available ?? true);

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

  // ── state mutation (owned by the composing context's loaders) ────
  function setStates(next: SettingStateResponse[]): void {
    states = next;
  }

  function setBusy(value: boolean): void {
    busy = value;
  }

  function setLoadError(value: string | null): void {
    loadError = value;
  }

  function clearAll(): void {
    states = [];
    loadError = null;
    busy = false;
    pending.clear();
  }

  function patch(updated: SettingStateResponse): void {
    states = states.map((s) => (s.setting_key === updated.setting_key ? updated : s));
  }

  function reportActionError(label: string, error: unknown): void {
    reportClientError('nvapi_settings_action', error);
    publishPresentedErrorNotification(label, error);
  }

  function ensureElevated(): boolean {
    if (isElevated()) {
      return true;
    }
    reportActionError(t('nvidia.adminRequired'), new ClientError('nvapi_admin_required'));
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

  return {
    // state accessors
    get states() {
      return states;
    },
    get hasStates() {
      return hasStates;
    },
    get busy() {
      return busy;
    },
    get loadError() {
      return loadError;
    },
    get nvapiAvailable() {
      return nvapiAvailable;
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
    // mutation / actions
    setStates,
    setBusy,
    setLoadError,
    clearAll,
    patch,
    reportActionError,
    ensureElevated,
    runWrite,
  };
}
