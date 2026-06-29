import { describeCommandErrorTechnical } from '@shared/api';
import { t } from '@shared/i18n';
import { publishErrorNotification } from '@shared/notifications';
import { clearDownloadProgress } from '@entities/library';

import { renodxApi, type RenoDxApi } from '../api/desktop';
import type {
  AvailabilityOutcome,
  HostKind,
  ManualFileInstall,
  MatchConfidence,
  RenoDxAddonState,
  RenoDxFreshness,
  ReshadeHost,
  ReshadeHostAction,
  RenoDxInstallState,
  RenoDxUpdateReport,
  ReshadeChannel,
  ReshadeHostOwnership,
  RiskAssessment,
  VulkanLayerStatus,
} from './types';

/** Reactive store backing the RenoDX card for a single game. */
export type RenoDxStore = ReturnType<typeof createRenoDxStore>;

type AvailabilitySnapshot = {
  reshadeHost: ReshadeHost;
  reshadeHostAction: ReshadeHostAction;
  reshadeConflict: boolean;
  reshadeChannel: ReshadeChannel | null;
  reshadeStableSupported: boolean;
  reshadeOwnership: ReshadeHostOwnership;
  renodxAddon: RenoDxAddonState | null;
};

function availabilitySnapshotFromReport(report: {
  reshade_host: ReshadeHost;
  reshade_host_action: ReshadeHostAction;
  reshade_conflict: boolean;
  reshade_channel: ReshadeChannel | null;
  reshade_stable_supported: boolean;
  reshade_ownership: ReshadeHostOwnership;
  renodx_addon: RenoDxAddonState | null;
}): AvailabilitySnapshot {
  return {
    reshadeHost: report.reshade_host,
    reshadeHostAction: report.reshade_host_action,
    reshadeConflict: report.reshade_conflict,
    reshadeChannel: report.reshade_channel,
    reshadeStableSupported: report.reshade_stable_supported,
    reshadeOwnership: report.reshade_ownership,
    renodxAddon: report.renodx_addon,
  };
}

/**
 * Maps the probe state + update report to the single freshness verdict the card
 * renders as a pill. A pure function (exported for unit tests). Order matters:
 * - a probe in flight wins, suppressing a transient verdict;
 * - a failed probe reads `unknown` — it writes the same `{ addon: null, host: null }`
 *   report as a successful *untracked* probe, so it must be ruled out before the
 *   `untracked` check;
 * - `available` (some source changed) outranks the per-source breakdown.
 */
export function deriveFreshness(
  updateProbing: boolean,
  probeFailed: boolean,
  updateReport: RenoDxUpdateReport | null,
): RenoDxFreshness {
  if (updateProbing) {
    return 'checking';
  }
  if (probeFailed || updateReport === null) {
    return 'unknown';
  }
  if (updateReport.overall === 'available') {
    return 'available';
  }
  if (updateReport.addon === null && updateReport.host === null) {
    return 'untracked';
  }
  if (updateReport.overall === 'current') {
    return 'current';
  }
  return 'unknown';
}

/**
 * Creates the RenoDX store. The backend API is injected so tests can drive the
 * store with fakes; production code uses the default Tauri-bound [`renodxApi`].
 */
export function createRenoDxStore(api: RenoDxApi = renodxApi) {
  let state = $state<RenoDxInstallState | null>(null);
  let availabilitySnapshot = $state<AvailabilitySnapshot>({
    reshadeHost: { status: 'absent' },
    reshadeHostAction: 'conflict',
    reshadeConflict: false,
    reshadeChannel: null,
    reshadeStableSupported: true,
    reshadeOwnership: { kind: 'missing' },
    renodxAddon: null,
  });
  let selectedReshadeChannel = $state<ReshadeChannel>('stable');
  let outcome = $state<AvailabilityOutcome | null>(null);
  let manualInstall = $state<ManualFileInstall | null>(null);
  // Global ReShade Vulkan layer status (from the availability preview); drives the
  // consent prompt for a Vulkan install and the layer-management note.
  let vulkanLayer = $state<VulkanLayerStatus | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let busy = $state(false);
  let loadError = $state<string | null>(null);
  // Upstream-update status of the installed add-on; `null` until checked (or
  // when nothing is installed).
  let updateReport = $state<RenoDxUpdateReport | null>(null);
  // True while an upstream-update probe is in flight (after `load`). Distinguishes
  // "still checking" from "checked and neither source is tracked" so the card
  // does not flash a transient "Updates not tracked" verdict.
  let updateProbing = $state(false);
  // True when the last update probe failed (network). Without it, the failure
  // verdict — `{ addon: null, host: null, … }` — is indistinguishable from a
  // successful probe of an untracked (file + foreign host) install, so the card
  // would mislabel a network failure as "Updates not tracked".
  let probeFailed = $state(false);
  // Wall-clock time of the last completed update probe (load or manual check), in
  // epoch ms; `null` until the first probe finishes. Drives the "Last checked …"
  // line so the user can see how fresh the verdict is.
  let lastCheckedAt = $state<number | null>(null);

  // Monotonic token: each load() captures one, and only the latest is allowed
  // to commit its result. Guards against a slow earlier load (e.g. the user
  // switched games quickly) overwriting a newer game's state.
  let requestId = 0;

  const isInstalled = $derived(state?.status === 'installed');
  const isManaged = $derived(state?.status === 'installed' && state.reshade_managed_by_us);
  const isInstallable = $derived(outcome?.kind === 'installable');
  const isExternal = $derived(outcome?.kind === 'external');
  const isNativeHdr = $derived(outcome?.kind === 'native_hdr');
  const isBlacklisted = $derived(outcome?.kind === 'blacklisted');
  const isUnsupported = $derived(outcome?.kind === 'unsupported');
  const isIncompatible = $derived(outcome?.kind === 'incompatible');
  const confidence = $derived<MatchConfidence | null>(
    outcome?.kind === 'installable' ? outcome.confidence : null,
  );
  const notesKeys = $derived<string[]>(outcome?.kind === 'installable' ? outcome.notes_keys : []);
  const externalUrl = $derived(outcome?.kind === 'external' ? outcome.url : null);
  const externalLabelKey = $derived(outcome?.kind === 'external' ? outcome.label_key : null);
  // File-install offer for a compatible external game (`null` when link-only).
  const externalFileInstall = $derived(outcome?.kind === 'external' ? outcome.file_install : null);
  const externalFileInstallable = $derived(externalFileInstall !== null);
  const externalConfidence = $derived<MatchConfidence | null>(
    externalFileInstall?.confidence ?? null,
  );
  const externalRisk = $derived<RiskAssessment | null>(externalFileInstall?.risk ?? null);
  const externalNotes = $derived<string[]>(externalFileInstall?.notes_keys ?? []);
  const externalRequiresConfirmation = $derived(externalRisk?.severity === 'warn');
  const externalIsBlocked = $derived(externalRisk?.severity === 'block');
  // How RenoDX would hook in for the resolved install / external file-install.
  const installHostKind = $derived<HostKind | null>(
    outcome?.kind === 'installable' ? outcome.host_kind : null,
  );
  const externalHostKind = $derived<HostKind | null>(externalFileInstall?.host_kind ?? null);
  // A Vulkan install needs consent to add the global ReShade Vulkan layer first, but
  // only when none is present yet (a foreign/managed layer is reused without consent).
  const vulkanConsentNeeded = $derived(
    (installHostKind === 'vulkan' || externalHostKind === 'vulkan') && vulkanLayer === 'absent',
  );
  const blacklistReason = $derived(outcome?.kind === 'blacklisted' ? outcome.reason : null);
  const risk = $derived<RiskAssessment | null>(
    outcome?.kind === 'installable' ? outcome.risk : null,
  );
  const requiresConfirmation = $derived(risk?.severity === 'warn');
  const isBlocked = $derived(risk?.severity === 'block');
  const updateAvailable = $derived(updateReport?.overall === 'available');
  const addonUpdate = $derived(updateReport?.addon ?? null);
  const hostUpdate = $derived(updateReport?.host ?? null);
  const dlssFixUpdate = $derived(updateReport?.dlssFix ?? null);
  // Whether the install includes the DLSS-Fix companion. Read straight off the
  // install state (the backend records a DlssFix tracked source) so this stays
  // correct even while the update probe is in flight or after it failed — the
  // update report's `dlssFix` is `null` in both cases and can't be relied on.
  const dlssFixInstalled = $derived(state?.status === 'installed' && state.dlss_fix_installed);
  let dlssFixAvailable = $state(false);

  // The upstream "Add-on dated …" anchor and install/update timestamps, surfaced
  // only when installed.
  const addonDated = $derived(state?.status === 'installed' ? state.addon_dated : null);
  const installedAt = $derived(state?.status === 'installed' ? state.installed_at : null);
  const updatedAt = $derived(state?.status === 'installed' ? state.updated_at : null);
  // Whether the add-on is an upstream-tracked install (vs a user-file install).
  // Read off the state, so the "installed from a file" hint is authoritative —
  // not inferred from the update report, which is `null` mid-probe / on failure.
  const addonTracked = $derived(state?.status === 'installed' ? state.addon_tracked : null);

  // The single freshness verdict the card renders as a status pill. The mapping
  // logic lives in the pure, unit-tested `deriveFreshness` above.
  const freshness = $derived.by(() => deriveFreshness(updateProbing, probeFailed, updateReport));

  function applyAvailabilitySnapshot(
    report: Parameters<typeof availabilitySnapshotFromReport>[0],
    mode: 'resetSelection' | 'preserveSelection',
  ): void {
    const nextSnapshot = availabilitySnapshotFromReport(report);
    availabilitySnapshot = nextSnapshot;
    if (nextSnapshot.reshadeChannel) {
      selectedReshadeChannel = nextSnapshot.reshadeChannel;
    } else if (mode === 'resetSelection') {
      selectedReshadeChannel = nextSnapshot.reshadeStableSupported ? 'stable' : 'nightly';
    } else if (!nextSnapshot.reshadeStableSupported && selectedReshadeChannel === 'stable') {
      selectedReshadeChannel = 'nightly';
    }
  }

  /**
   * Loads the current install state and availability for `gameId`. Only the
   * most recent invocation commits its result; a stale response (a newer load
   * has since started) is discarded so it cannot clobber fresher state.
   *
   * `loading` flips false as soon as availability resolves (before the update
   * probe), so the card renders immediately; the probe then resolves the
   * update verdict and DLSS-Fix availability. `busy` is never set by `load`,
   * so an in-flight probe never blocks mutations.
   */
  async function load(gameId: string): Promise<void> {
    const token = ++requestId;
    loading = true;
    loadError = null;
    updateReport = null;
    updateProbing = false;
    probeFailed = false;
    lastCheckedAt = null;
    dlssFixAvailable = false;
    try {
      const report = await api.getAvailability(gameId);
      if (token !== requestId) {
        return;
      }
      state = report.state;
      applyAvailabilitySnapshot(report, 'resetSelection');
      outcome = report.outcome;
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
      loaded = true;
    } catch (error) {
      if (token !== requestId) {
        return;
      }
      loadError = describeCommandErrorTechnical(error);
      publishErrorNotification(t('gameDetails.renodx.loadFailed'), loadError);
    } finally {
      if (token === requestId) {
        loading = false;
      }
    }

    // The token inside probeUpdateStatus discards a stale result if a newer
    // load/mutation started while the probe was in flight.
    await probeUpdateStatus(gameId, token);
  }

  /**
   * Refreshes state after a successful mutation **without** re-fetching
   * availability or probing for updates.
   *
   * Installability (`outcome`) does not change when a game is installed or
   * uninstalled, and re-fetching it would re-read the game executable and
   * re-resolve the manifest for nothing. The upstream-update probe is skipped
   * too: we just installed/updated, so every tracked source is current by
   * construction (running the probe would needlessly re-download the add-on
   * and ReShade host to compare digests). `nextState` carries the new install
   * state (including `dlss_fix_installed`, read straight off it), so
   * `dlssFixInstalled` and the DLSS-Fix-availability probe stay correct.
   *
   * Synchronous: it does no awaiting of its own. The only async work — the
   * best-effort DLSS-Fix-availability probe — is fired in the background and
   * guarded by `token`, so the mutation's `busy` flag clears as soon as the
   * state is applied rather than blocking on a network round-trip.
   */
  function refreshAfterMutation(gameId: string, nextState: RenoDxInstallState): void {
    const token = ++requestId;
    const stamped = stampMutationTimestamps(nextState, state);
    state = stamped;
    loading = false;
    loadError = null;
    updateProbing = false;
    probeFailed = false;
    updateReport = updateReportForInstall(stamped);
    // We just installed/updated, so the verdict is current as of now.
    lastCheckedAt = stamped.status === 'installed' ? Date.now() : null;
    dlssFixAvailable = false;
    // `outcome` is left as-is: installability is independent of the current
    // install state, so the last load's verdict is still valid.

    // The host the slot loads *does* change on install/uninstall/replace, so the
    // host state (version / add-on support / action / conflict / add-on config)
    // must be re-read — otherwise a fresh install keeps the pre-install `Absent`
    // host and the panel shows "version unknown · add-on support unknown". This is
    // a local scan (no upstream probe); it does not touch the optimistic `state`.
    void refreshHostInfo(gameId, token);

    // Offer to install a DLSS-Fix only when RenoDX is installed without one.
    if (stamped.status === 'installed' && !stamped.dlss_fix_installed) {
      void probeDlssFixAvailability(gameId, token);
    }
  }

  /**
   * Re-reads the ReShade host state after a mutation from a fresh availability
   * scan (local, no upstream probe). Best-effort and token-guarded; a stale result
   * (a newer load/mutation started meanwhile) is discarded, and the optimistic
   * install `state` set by [`refreshAfterMutation`] is left untouched.
   */
  async function refreshHostInfo(gameId: string, token: number): Promise<void> {
    try {
      const report = await api.getAvailability(gameId);
      if (token === requestId) {
        applyAvailabilitySnapshot(report, 'preserveSelection');
      }
    } catch {
      // Best-effort: a failed host refresh leaves the optimistic state in place.
    }
  }

  /**
   * Fills the optimistic install/update timestamps a mutation's returned state
   * lacks (the backend builds it from an in-memory record, so its timestamps and
   * `addon_dated` are `null` until the next `load` re-reads them from the DB). A
   * fresh install stamps both `installed_at` and `updated_at` to now; an update
   * (the game was already installed) preserves the prior `installed_at` and only
   * bumps `updated_at`. `addon_dated` is intentionally left as-is — its real value
   * arrives on the next load — so the card falls back to "Installed …" meanwhile.
   */
  function stampMutationTimestamps(
    nextState: RenoDxInstallState,
    priorState: RenoDxInstallState | null,
  ): RenoDxInstallState {
    if (nextState.status !== 'installed') {
      return nextState;
    }
    const prior = priorState?.status === 'installed' ? priorState : null;
    const now = Date.now();
    return {
      ...nextState,
      installed_at: nextState.installed_at ?? prior?.installed_at ?? now,
      updated_at: nextState.updated_at ?? now,
    };
  }

  /** Best-effort DLSS-Fix-availability probe. A stale token is discarded. */
  async function probeDlssFixAvailability(gameId: string, token: number): Promise<void> {
    try {
      const available = await api.dlssFixAvailability(gameId);
      if (token === requestId) {
        dlssFixAvailable = available;
      }
    } catch {
      if (token === requestId) {
        dlssFixAvailable = false;
      }
    }
  }

  /**
   * The update report assumed right after a mutation: every tracked source is
   * current (we just installed/updated it), and `dlssFix` mirrors the state's
   * `dlss_fix_installed` so `dlssFixUpdate` stays consistent. `null` overall
   * when the game is no longer installed (nothing to track).
   */
  function updateReportForInstall(nextState: RenoDxInstallState): RenoDxUpdateReport | null {
    if (nextState.status !== 'installed') {
      return null;
    }
    return {
      addon: 'current',
      host: 'current',
      dlssFix: nextState.dlss_fix_installed ? 'current' : null,
      overall: 'current',
    };
  }

  /**
   * Best-effort upstream-update probe after a load. A network failure yields
   * `unknown`, never a load error, and a stale token is discarded. Also probes
   * DLSS-Fix availability when RenoDX is installed without one. Sets
   * `updateProbing` for the duration so the card can suppress a transient
   * "Updates not tracked" verdict until the result lands.
   */
  async function probeUpdateStatus(gameId: string, token: number): Promise<void> {
    if (token !== requestId || state?.status !== 'installed') {
      return;
    }
    updateProbing = true;
    probeFailed = false;
    try {
      const report = await api.checkUpdate(gameId);
      if (token === requestId) {
        updateReport = report;
      }
    } catch {
      if (token === requestId) {
        updateReport = { addon: null, host: null, dlssFix: null, overall: 'unknown' };
        probeFailed = true;
      }
    } finally {
      if (token === requestId) {
        updateProbing = false;
        lastCheckedAt = Date.now();
      }
    }
    // DLSS-Fix is offered only when RenoDX is installed and no DLSS-Fix is tracked.
    // Reset before probing so a stale "available" never lingers once one is installed.
    if (token === requestId) {
      dlssFixAvailable = false;
    }
    if (token === requestId && updateReport?.dlssFix === null) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  /**
   * User-initiated "check for updates": re-runs the upstream probe for the
   * installed game and re-stamps `lastCheckedAt`. A no-op when nothing is
   * installed (the button is only shown when installed). Never sets `busy`, so it
   * does not block mutations; a stale token is discarded inside the probe.
   */
  async function checkForUpdates(gameId: string): Promise<void> {
    const token = ++requestId;
    await probeUpdateStatus(gameId, token);
  }

  /**
   * Marks the global Vulkan layer as ours after a consented Vulkan install added it,
   * so the card reflects the new state without waiting for a reload.
   */
  function notePossibleVulkanLayerInstall(confirmVulkanLayer: boolean): void {
    if (confirmVulkanLayer && vulkanLayer === 'absent') {
      vulkanLayer = 'managed';
    }
  }

  /**
   * Installs RenoDX, then refreshes state. `confirmAnticheat` gates the warn case;
   * `confirmVulkanLayer` consents to adding the global ReShade Vulkan layer for a
   * Vulkan game when none is present yet. Returns whether the install succeeded.
   */
  async function install(
    gameId: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
    confirmVulkanLayer = false,
  ): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    clearDownloadProgress([gameId]);
    try {
      const nextState = await api.install(gameId, channel, confirmAnticheat, confirmVulkanLayer);
      selectedReshadeChannel = channel;
      refreshAfterMutation(gameId, nextState);
      notePossibleVulkanLayerInstall(confirmVulkanLayer);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.installError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /**
   * Installs RenoDX for an external game from a user-downloaded add-on file, then
   * refreshes state. `confirmAnticheat` gates the warn case; `confirmVulkanLayer`
   * consents to the global Vulkan layer for a Vulkan game. Returns success.
   */
  async function installFromFile(
    gameId: string,
    filePath: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
    confirmVulkanLayer = false,
  ): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    clearDownloadProgress([gameId]);
    try {
      const nextState = await api.installFromFile(
        gameId,
        filePath,
        channel,
        confirmAnticheat,
        confirmVulkanLayer,
      );
      selectedReshadeChannel = channel;
      refreshAfterMutation(gameId, nextState);
      notePossibleVulkanLayerInstall(confirmVulkanLayer);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.installError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /**
   * Updates the installed add-on to the latest upstream snapshot, then refreshes
   * state. A DLSS-Fix, if installed, is preserved across an update and reported
   * on the returned state's `dlss_fix_installed`. Returns whether it succeeded.
   */
  async function update(gameId: string): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    clearDownloadProgress([gameId]);
    try {
      const nextState = await api.update(gameId);
      refreshAfterMutation(gameId, nextState);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.updateError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /** Switches the managed ReShade host channel and keeps the add-on channel-pinned. */
  async function switchChannel(gameId: string, channel: ReshadeChannel): Promise<boolean> {
    if (busy || channel === availabilitySnapshot.reshadeChannel) {
      return false;
    }
    busy = true;
    clearDownloadProgress([gameId]);
    try {
      const nextState = await api.switchChannel(gameId, channel);
      selectedReshadeChannel = channel;
      availabilitySnapshot = { ...availabilitySnapshot, reshadeChannel: channel };
      refreshAfterMutation(gameId, nextState);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.switchError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  function setSelectedReshadeChannel(channel: ReshadeChannel): void {
    selectedReshadeChannel =
      channel === 'stable' && !availabilitySnapshot.reshadeStableSupported ? 'nightly' : channel;
  }

  /** Uninstalls RenoDX, then refreshes state. Returns whether it succeeded. */
  async function uninstall(gameId: string): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    try {
      const nextState = await api.uninstall(gameId);
      refreshAfterMutation(gameId, nextState);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.uninstallError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /** Installs the DLSS-Fix companion add-on, then refreshes state. */
  async function installDlssFix(gameId: string): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    clearDownloadProgress([gameId]);
    try {
      const nextState = await api.installDlssFix(gameId);
      refreshAfterMutation(gameId, nextState);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.dlssFixInstallError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /** Removes the DLSS-Fix companion add-on, then refreshes state. */
  async function uninstallDlssFix(gameId: string): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    try {
      const nextState = await api.uninstallDlssFix(gameId);
      refreshAfterMutation(gameId, nextState);
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.dlssFixRemoveError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  /**
   * Removes RenderPilot's global ReShade Vulkan layer (user maintenance). A foreign
   * layer is never touched. Updates the reported status. Returns success.
   */
  async function removeVulkanLayer(): Promise<boolean> {
    if (busy) {
      return false;
    }
    busy = true;
    try {
      vulkanLayer = await api.removeVulkanLayer();
      return true;
    } catch (error) {
      publishErrorNotification(
        t('gameDetails.renodx.vulkanLayer.removeError'),
        describeCommandErrorTechnical(error),
      );
      return false;
    } finally {
      busy = false;
    }
  }

  return {
    get state() {
      return state;
    },
    get reshadeHost() {
      return availabilitySnapshot.reshadeHost;
    },
    get reshadeHostAction() {
      return availabilitySnapshot.reshadeHostAction;
    },
    get reshadeConflict() {
      return availabilitySnapshot.reshadeConflict;
    },
    get reshadeChannel() {
      return availabilitySnapshot.reshadeChannel;
    },
    get reshadeStableSupported() {
      return availabilitySnapshot.reshadeStableSupported;
    },
    get reshadeOwnership() {
      return availabilitySnapshot.reshadeOwnership;
    },
    get selectedReshadeChannel() {
      return selectedReshadeChannel;
    },
    get renodxAddon() {
      return availabilitySnapshot.renodxAddon;
    },
    get outcome() {
      return outcome;
    },
    /** The manual file-install escape hatch (DirectX game, no auto/external path). */
    get manualInstall() {
      return manualInstall;
    },
    get loading() {
      return loading;
    },
    get loaded() {
      return loaded;
    },
    get busy() {
      return busy;
    },
    get loadError() {
      return loadError;
    },
    get isInstalled() {
      return isInstalled;
    },
    get isManaged() {
      return isManaged;
    },
    get isInstallable() {
      return isInstallable;
    },
    get isExternal() {
      return isExternal;
    },
    get isNativeHdr() {
      return isNativeHdr;
    },
    get isBlacklisted() {
      return isBlacklisted;
    },
    get isUnsupported() {
      return isUnsupported;
    },
    get isIncompatible() {
      return isIncompatible;
    },
    get confidence() {
      return confidence;
    },
    get notesKeys() {
      return notesKeys;
    },
    get externalUrl() {
      return externalUrl;
    },
    get externalLabelKey() {
      return externalLabelKey;
    },
    get externalFileInstallable() {
      return externalFileInstallable;
    },
    get externalConfidence() {
      return externalConfidence;
    },
    get externalRisk() {
      return externalRisk;
    },
    get externalNotes() {
      return externalNotes;
    },
    get externalRequiresConfirmation() {
      return externalRequiresConfirmation;
    },
    get externalIsBlocked() {
      return externalIsBlocked;
    },
    /** Global ReShade Vulkan layer status (null until the availability preview loads). */
    get vulkanLayer() {
      return vulkanLayer;
    },
    /** Whether installing this game needs the user to consent to the global Vulkan layer. */
    get vulkanConsentNeeded() {
      return vulkanConsentNeeded;
    },
    get blacklistReason() {
      return blacklistReason;
    },
    get risk() {
      return risk;
    },
    get requiresConfirmation() {
      return requiresConfirmation;
    },
    get isBlocked() {
      return isBlocked;
    },
    get updateStatus() {
      return updateReport?.overall ?? null;
    },
    get updateProbing() {
      return updateProbing;
    },
    get freshness() {
      return freshness;
    },
    get lastCheckedAt() {
      return lastCheckedAt;
    },
    get addonDated() {
      return addonDated;
    },
    get addonTracked() {
      return addonTracked;
    },
    get installedAt() {
      return installedAt;
    },
    get updatedAt() {
      return updatedAt;
    },
    get addonUpdate() {
      return addonUpdate;
    },
    get hostUpdate() {
      return hostUpdate;
    },
    get dlssFixUpdate() {
      return dlssFixUpdate;
    },
    get dlssFixInstalled() {
      return dlssFixInstalled;
    },
    get dlssFixAvailable() {
      return dlssFixAvailable;
    },
    get updateAvailable() {
      return updateAvailable;
    },
    load,
    checkForUpdates,
    install,
    installFromFile,
    setSelectedReshadeChannel,
    switchChannel,
    update,
    uninstall,
    installDlssFix,
    uninstallDlssFix,
    removeVulkanLayer,
  };
}
