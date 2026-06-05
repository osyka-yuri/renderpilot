import { SvelteSet } from 'svelte/reactivity';
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
import type { LibraryManifest, LibraryManifestEntry, LibraryState } from '@entities/library';
import {
  getLibrariesManifest,
  fetchLibrariesManifest,
  getLibraryStates,
  downloadLibrary,
  deleteLibrary,
} from '@entities/library';

type EntryAction = 'download' | 'delete';
type PendingEntryAction = { entryId: string; action: EntryAction } | null;
type ManifestLoader = () => Promise<LibraryManifest>;

type LoadLibrariesOptions = {
  mode: 'initial' | 'refresh';
  loadManifest: ManifestLoader;
  failureContext: string;
};

type RunExclusiveEntryActionOptions = {
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
  let pendingEntryAction = $state<PendingEntryAction>(null);
  let activeVendor = $state<Vendor>(DEFAULT_VENDOR);
  let activeType = $state<LibraryTypeValue>(DEFAULT_TYPE_BY_VENDOR[DEFAULT_VENDOR]);
  const lastTypeByVendor = $state<Record<Vendor, LibraryTypeValue>>({ ...DEFAULT_TYPE_BY_VENDOR });

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  let mounted = false;
  let loadRequestId = 0;

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  const isBusy = $derived(loading || refreshing || pendingEntryAction !== null);
  const downloadedEntryIds = $derived(createDownloadedEntryIdSet(states));
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
      failureContext: 'Failed to load libraries',
    });
  }

  async function refreshManifest(): Promise<void> {
    if (isBusy) return;

    await loadLibraries({
      mode: 'refresh',
      loadManifest: fetchLibrariesManifest,
      failureContext: 'Failed to refresh manifest',
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
      states = nextStates;
    } catch (error) {
      if (!isCurrentLoadRequest(requestId)) return;

      if (isInitialLoad) {
        manifest = null;
        states = [];
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

  async function handleDownload(entryId: string): Promise<void> {
    await runExclusiveEntryAction({
      entryId,
      action: 'download',
      failureContext: 'Download failed',
      refreshFailureContext: 'Library downloaded, but status refresh failed',
      execute: downloadLibrary,
    });
  }

  async function handleDelete(entryId: string): Promise<void> {
    await runExclusiveEntryAction({
      entryId,
      action: 'delete',
      failureContext: 'Delete failed',
      refreshFailureContext: 'Library deleted, but status refresh failed',
      execute: deleteLibrary,
    });
  }

  async function runExclusiveEntryAction(options: RunExclusiveEntryActionOptions): Promise<void> {
    if (isBusy) return;

    pendingEntryAction = { entryId: options.entryId, action: options.action };
    errorMessage = null;

    try {
      await options.execute(options.entryId);

      if (!mounted) return;

      await refreshLibraryStates(options.refreshFailureContext);
    } catch (error) {
      if (mounted) {
        setError(options.failureContext, error);
      }
      throw error;
    } finally {
      if (mounted) {
        pendingEntryAction = null;
      }
    }
  }

  async function refreshLibraryStates(errorContext: string): Promise<void> {
    try {
      const nextStates = await getLibraryStates();

      if (mounted) {
        states = nextStates;
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

  function createDownloadedEntryIdSet(currentStates: LibraryState[]): ReadonlySet<string> {
    return new SvelteSet(
      currentStates.filter((state) => state.is_downloaded).map((state) => state.id),
    );
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
    if (isLoading) return 'Loading...';
    if (currentManifest === null && currentError !== null) return 'Unable to load libraries';
    if (entryCount === 0) return 'No libraries found';

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
    get pendingEntryAction() {
      return pendingEntryAction;
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
    get downloadedEntryIds() {
      return downloadedEntryIds;
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
