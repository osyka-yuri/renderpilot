import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import {
  vendorOptions,
  typeOptionsByVendor,
  groupKeyForType,
  libraryIdToGroupKey,
  getDefaultTypeForVendor,
  isVendor,
  type Vendor,
  type LibraryTypeValue,
} from './libraries-page-model';
import { describeCommandError } from '@shared/api';
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
};

const DEFAULT_VENDOR = vendorOptions[0].value;

const DEFAULT_TYPE_BY_VENDOR = Object.freeze(
  Object.fromEntries(
    vendorOptions.map((vendor) => [vendor.value, getDefaultTypeForVendor(vendor.value)]),
  ),
) as Readonly<Record<Vendor, LibraryTypeValue>>;

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

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  let mounted = false;
  let loadRequestId = 0;
  let statesRequestId = 0;

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  const isBusy = $derived(loading || refreshing || pendingActions.size > 0);
  const activeGroupKey = $derived(groupKeyForType(activeType));
  const filteredEntries = $derived(filterEntriesByGroup(manifest, activeGroupKey));
  const emptyMessage = $derived(
    getEmptyMessage(loading, manifest, errorMessage, filteredEntries.length),
  );

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
        setError(options.failureContext, error);
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

    // Actions
    loadInitialLibraries,
    refreshManifest,
    handleVendorChange,
    handleTypeChange,
    handleDownload,
    handleDelete,

    // Lifecycle
    init,
    dispose,
  };
}
