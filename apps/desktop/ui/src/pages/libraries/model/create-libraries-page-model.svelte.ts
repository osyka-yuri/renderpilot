import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import {
  vendorOptions,
  typeOptionsByVendor,
  groupKeyForType,
  libraryIdToGroupKey,
  selectLatestStableEntries,
  getDefaultTypeForVendor,
  isVendor,
  type Vendor,
  type LibraryTypeValue,
} from './libraries-page-model';
import { describeCommandError } from '@shared/api';
import { runWithConcurrency } from '@shared/concurrency';
import { t } from '@shared/i18n';
import {
  type LibraryManifest,
  type LibraryManifestEntry,
  type LibraryState,
  getLibrariesManifest,
  fetchLibrariesManifest,
  getLibraryStates,
  downloadLibrary,
  deleteLibrary,
  clearDownloadProgress,
  sumDownloadFractions,
} from '@entities/library';

type EntryAction = 'download' | 'delete';
type ManifestLoader = () => Promise<LibraryManifest>;

type LoadLibrariesOptions = {
  mode: 'initial' | 'refresh';
  loadManifest: ManifestLoader;
  failureContext: string;
};

type RunEntryActionOptions = {
  entryId: string;
  action: EntryAction;
  failureContext: string;
  refreshFailureContext: string;
  execute: (entryId: string) => Promise<unknown>;
  // When true, a failure is logged but not surfaced in the page-level error
  // banner. Bulk runs report a single summary toast instead of N banner errors.
  suppressErrorBanner?: boolean;
};

const DEFAULT_VENDOR = vendorOptions[0].value;

const DEFAULT_TYPE_BY_VENDOR = Object.freeze(
  Object.fromEntries(
    vendorOptions.map((vendor) => [vendor.value, getDefaultTypeForVendor(vendor.value)]),
  ),
) as Readonly<Record<Vendor, LibraryTypeValue>>;

/** Outcome of a "download all latest" run, aggregated across every target. */
export type BulkDownloadResult = Readonly<{
  succeeded: number;
  failed: number;
  skipped: number;
}>;

const EMPTY_BULK_RESULT: BulkDownloadResult = { succeeded: 0, failed: 0, skipped: 0 };

// Download a few entries at once: faster than serial without saturating the
// connection. Each entry still streams its own per-row progress.
const BULK_DOWNLOAD_CONCURRENCY = 3;

export type LibrariesPageModel = ReturnType<typeof createLibrariesPageModel>;

export function createLibrariesPageModel() {
  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let manifest = $state<LibraryManifest | null>(null);
  let states = $state<LibraryState[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let errorMessage = $state<string | null>(null);
  // One in-flight action per entry; actions on different entries run concurrently.
  const pendingActions = new SvelteMap<string, EntryAction>();
  // Stable reactive set (synced from `states`) so the static table columns can
  // read per-row membership without ever being recreated.
  const downloadedEntryIds = new SvelteSet<string>();
  let activeVendor = $state<Vendor>(DEFAULT_VENDOR);
  let activeType = $state<LibraryTypeValue>(DEFAULT_TYPE_BY_VENDOR[DEFAULT_VENDOR]);
  const lastTypeByVendor = $state<Record<Vendor, LibraryTypeValue>>({ ...DEFAULT_TYPE_BY_VENDOR });
  // "Download all latest" progress; meaningful only while `bulkDownloading`.
  let bulkDownloading = $state(false);
  let bulkTotal = $state(0);
  let bulkCompleted = $state(0);
  // Snapshot of the batch's entry ids, taken at start: `targets` would otherwise
  // shrink mid-run as `downloadedEntryIds` updates. Used to aggregate progress.
  let bulkTargetIds = $state<readonly string[]>([]);

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  let mounted = false;
  let loadRequestId = 0;
  let statesRequestId = 0;

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  const isBusy = $derived(loading || refreshing || pendingActions.size > 0 || bulkDownloading);
  const activeGroupKey = $derived(groupKeyForType(activeType));
  const filteredEntries = $derived(filterEntriesByGroup(manifest, activeGroupKey));
  const emptyMessage = $derived(
    getEmptyMessage(loading, manifest, errorMessage, filteredEntries.length),
  );
  // Newest stable build of every library (Streamline bundle handled as a set),
  // and how many of those still need downloading.
  const latestStableEntries = $derived(selectLatestStableEntries(manifest));
  const latestStablePendingCount = $derived(
    latestStableEntries.filter((entry) => !downloadedEntryIds.has(entry.entry_id)).length,
  );
  // Smooth aggregate for the bulk bar: finished entries count as 1, in-flight
  // ones add their own byte fraction, so the bar advances even within a single
  // large download. In-flight entries are identified via `pendingActions`,
  // intersected with the batch snapshot so a stray single-row download can't
  // leak into the sum; finished entries are already covered by `bulkCompleted`,
  // so there is no double counting.
  const bulkProgressValue = $derived.by(() => {
    if (!bulkDownloading) return 0;
    const inFlight = bulkTargetIds.filter((id) => pendingActions.get(id) === 'download');
    return bulkCompleted + sumDownloadFractions(inFlight);
  });

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  async function loadInitialLibraries(): Promise<void> {
    await loadLibraries({
      mode: 'initial',
      loadManifest: getLibrariesManifest,
      failureContext: t('libraries.error.loadFailed'),
    });
  }

  async function refreshManifest(): Promise<void> {
    if (isBusy) return;

    await loadLibraries({
      mode: 'refresh',
      loadManifest: fetchLibrariesManifest,
      failureContext: t('libraries.error.refreshFailed'),
    });
  }

  async function loadLibraries(options: LoadLibrariesOptions): Promise<void> {
    const requestId = ++loadRequestId;
    const isInitialLoad = options.mode === 'initial';

    if (isInitialLoad) {
      loading = true;
    } else {
      refreshing = true;
    }

    errorMessage = null;

    try {
      const [nextManifest, nextStates] = await Promise.all([
        options.loadManifest(),
        getLibraryStates(),
      ]);

      if (!isCurrentLoadRequest(requestId)) return;

      manifest = nextManifest;
      setStates(nextStates);
    } catch (error) {
      if (!isCurrentLoadRequest(requestId)) return;

      if (isInitialLoad) {
        manifest = null;
        setStates([]);
      }

      setError(options.failureContext, error);
    } finally {
      if (isCurrentLoadRequest(requestId)) {
        loading = false;
        if (!isInitialLoad) {
          refreshing = false;
        }
      }
    }
  }

  function handleVendorChange(value: unknown): void {
    if (typeof value !== 'string' || !isVendor(value)) return;

    activeVendor = value;
    activeType = getLastValidTypeForVendor(value);
  }

  function handleTypeChange(value: unknown): void {
    if (typeof value !== 'string' || !isLibraryTypeForVendor(activeVendor, value)) {
      return;
    }

    activeType = value;
    lastTypeByVendor[activeVendor] = value;
  }

  async function handleDownload(entryId: string): Promise<boolean> {
    return runEntryAction({
      entryId,
      action: 'download',
      failureContext: t('libraries.error.downloadFailed'),
      refreshFailureContext: t('libraries.error.downloadedRefreshFailed'),
      execute: downloadLibrary,
    });
  }

  async function handleDelete(entryId: string): Promise<boolean> {
    return runEntryAction({
      entryId,
      action: 'delete',
      failureContext: t('libraries.error.deleteFailed'),
      refreshFailureContext: t('libraries.error.deletedRefreshFailed'),
      execute: deleteLibrary,
    });
  }

  /**
   * Downloads the newest stable build of every library that isn't downloaded
   * yet (see {@link selectLatestStableEntries}). Reuses the per-entry pipeline,
   * so each affected row lights up its own spinner/progress; this returns an
   * aggregate the caller turns into a single summary toast.
   *
   * A failing entry never aborts the rest — failures are counted, not thrown.
   */
  async function downloadAllLatest(): Promise<BulkDownloadResult> {
    if (isBusy) return EMPTY_BULK_RESULT;

    const targets = latestStableEntries.filter((entry) => !downloadedEntryIds.has(entry.entry_id));
    if (targets.length === 0) return EMPTY_BULK_RESULT;

    bulkDownloading = true;
    bulkTotal = targets.length;
    bulkCompleted = 0;
    bulkTargetIds = targets.map((entry) => entry.entry_id);
    errorMessage = null;

    let succeeded = 0;
    let failed = 0;
    let skipped = 0;

    try {
      await runWithConcurrency(targets, BULK_DOWNLOAD_CONCURRENCY, async (entry) => {
        try {
          const ran = await runEntryAction({
            entryId: entry.entry_id,
            action: 'download',
            failureContext: t('libraries.error.downloadFailed'),
            refreshFailureContext: t('libraries.error.downloadedRefreshFailed'),
            execute: downloadLibrary,
            // The batch reports a single summary toast; per-entry failures must
            // not also pile up in (and flicker) the page-level error banner.
            suppressErrorBanner: true,
          });
          if (ran) {
            succeeded += 1;
          } else {
            skipped += 1;
          }
        } catch {
          // Already logged by `runEntryAction`; keep going so one bad entry
          // can't strand the rest of the batch.
          failed += 1;
        } finally {
          bulkCompleted += 1;
        }
      });
    } finally {
      bulkDownloading = false;
      bulkTotal = 0;
      bulkCompleted = 0;
      bulkTargetIds = [];
    }

    return { succeeded, failed, skipped };
  }

  /**
   * Runs a download/delete for one entry. Entries are independent: actions on
   * different entries run concurrently, while a second action on the same
   * entry (or any action during a manifest load/refresh) is ignored.
   *
   * Returns `true` only when the action actually ran — callers must not
   * report success otherwise.
   */
  async function runEntryAction(options: RunEntryActionOptions): Promise<boolean> {
    if (loading || refreshing || pendingActions.has(options.entryId)) return false;

    pendingActions.set(options.entryId, options.action);
    if (options.action === 'download') {
      clearDownloadProgress([options.entryId]);
    }
    errorMessage = null;

    try {
      await options.execute(options.entryId);

      if (mounted) {
        await refreshLibraryStates(options.refreshFailureContext);
      }
      return true;
    } catch (error) {
      if (mounted) {
        if (options.suppressErrorBanner) {
          console.error(`${options.failureContext}:`, error);
        } else {
          setError(options.failureContext, error);
        }
      }
      throw error;
    } finally {
      pendingActions.delete(options.entryId);
    }
  }

  async function refreshLibraryStates(errorContext: string): Promise<void> {
    // Concurrent downloads finish independently; the counter makes sure a
    // slower, older snapshot can never overwrite a fresher one.
    const requestId = ++statesRequestId;

    try {
      const nextStates = await getLibraryStates();

      if (mounted && requestId === statesRequestId) {
        setStates(nextStates);
      }
    } catch (error) {
      if (mounted) {
        setError(errorContext, error);
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  function init(): void {
    mounted = true;
  }

  function dispose(): void {
    mounted = false;
    loadRequestId += 1;
  }

  // ---------------------------------------------------------------------------
  // Pure helpers
  // ---------------------------------------------------------------------------

  function filterEntriesByGroup(
    currentManifest: LibraryManifest | null,
    groupKey: ReturnType<typeof groupKeyForType>,
  ): LibraryManifestEntry[] {
    return (
      currentManifest?.entries.filter(
        (entry) => libraryIdToGroupKey(entry.library.id) === groupKey,
      ) ?? []
    );
  }

  function setStates(nextStates: LibraryState[]): void {
    states = nextStates;
    syncDownloadedEntryIds(nextStates);
  }

  /**
   * Mirrors `states` into the stable `downloadedEntryIds` set with a minimal
   * diff, so only rows whose download status actually changed re-render.
   */
  function syncDownloadedEntryIds(currentStates: LibraryState[]): void {
    const nextIds = currentStates.filter((state) => state.is_downloaded).map((state) => state.id);

    for (const id of downloadedEntryIds) {
      if (!nextIds.includes(id)) {
        downloadedEntryIds.delete(id);
      }
    }

    for (const id of nextIds) {
      downloadedEntryIds.add(id);
    }
  }

  function getLastValidTypeForVendor(vendor: Vendor): LibraryTypeValue {
    const storedType = lastTypeByVendor[vendor];

    if (isLibraryTypeForVendor(vendor, storedType)) {
      return storedType;
    }

    const fallbackType = getDefaultTypeForVendor(vendor);
    lastTypeByVendor[vendor] = fallbackType;
    return fallbackType;
  }

  function isLibraryTypeForVendor(vendor: Vendor, value: string): value is LibraryTypeValue {
    return typeOptionsByVendor[vendor].some((option) => option.value === value);
  }

  function getEmptyMessage(
    isLoading: boolean,
    currentManifest: LibraryManifest | null,
    currentError: string | null,
    entryCount: number,
  ): string | null {
    if (isLoading) return t('libraries.empty.loading');
    if (currentManifest === null && currentError !== null) return t('libraries.empty.unavailable');
    if (entryCount === 0) return t('libraries.empty.none');

    return null;
  }

  function isCurrentLoadRequest(requestId: number): boolean {
    return mounted && requestId === loadRequestId;
  }

  function setError(context: string, error: unknown): void {
    errorMessage = `${context}: ${describeCommandError(error)}`;
    console.error(`${context}:`, error);
  }

  // ---------------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------------

  return {
    // State (read-only)
    get manifest() {
      return manifest;
    },
    get states() {
      return states;
    },
    get loading() {
      return loading;
    },
    get refreshing() {
      return refreshing;
    },
    get errorMessage() {
      return errorMessage;
    },
    get pendingActions() {
      return pendingActions as ReadonlyMap<string, EntryAction>;
    },
    get downloadedEntryIds() {
      return downloadedEntryIds as ReadonlySet<string>;
    },
    get activeVendor() {
      return activeVendor;
    },
    get activeType() {
      return activeType;
    },
    set activeType(value: string | undefined) {
      handleTypeChange(value);
    },
    get bulkDownloading() {
      return bulkDownloading;
    },
    get bulkTotal() {
      return bulkTotal;
    },
    get bulkCompleted() {
      return bulkCompleted;
    },
    get bulkProgressValue() {
      return bulkProgressValue;
    },

    // Derived
    get isBusy() {
      return isBusy;
    },
    get activeGroupKey() {
      return activeGroupKey;
    },
    get filteredEntries() {
      return filteredEntries;
    },
    get emptyMessage() {
      return emptyMessage;
    },
    get latestStablePendingCount() {
      return latestStablePendingCount;
    },

    // Actions
    loadInitialLibraries,
    refreshManifest,
    handleVendorChange,
    handleTypeChange,
    handleDownload,
    handleDelete,
    downloadAllLatest,

    // Lifecycle
    init,
    dispose,
  };
}
