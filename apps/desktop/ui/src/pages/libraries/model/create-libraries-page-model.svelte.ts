import { SvelteMap } from 'svelte/reactivity';
import {
  vendorOptions,
  typeOptionsByVendor,
  filterPackageRows,
  selectLatestStablePackages,
  getDefaultTypeForVendor,
  isVendor,
  type Vendor,
  type LibraryTypeValue,
  type LibraryPackageRow,
  shouldShowPackageDisplayName,
  shouldDeleteLibraryPackage,
} from './libraries-page-model';
import { formatPresentedError } from '@shared/error-presentation';
import { reportClientError } from '@shared/errors';
import { runWithConcurrency } from '@shared/concurrency';
import { clearDownloadProgress, sumDownloadFractions } from '@shared/lib';
import { createDisposableRequestChannel } from '@shared/requests';
import { t } from '@shared/i18n';
import {
  type LibraryPackageMutation,
  type LibraryPackageSummary,
  type LibraryPackagesOutput,
  listLibraryPackages,
  downloadLibraryPackage,
  deleteLibraryPackage,
} from '@entities/library';
import { sharedLibraryPackagesCache, type LibraryPackagesCache } from './library-packages-cache';

type PackageAction = 'download' | 'delete';
type PackageActionOrigin = 'user' | 'bulk';

type LoadLibrariesOptions = {
  mode: 'initial' | 'refresh';
  failureContext: string;
};

type RunPackageActionOptions = {
  packageId: string;
  action: PackageAction;
  origin: PackageActionOrigin;
  failureContext: string;
  execute: () => Promise<LibraryPackageMutation>;
  // Bulk runs report one summary toast instead of N page-level errors.
  suppressErrorBanner?: boolean;
};

const DEFAULT_VENDOR = vendorOptions[0].value;

const DEFAULT_TYPE_BY_VENDOR = Object.freeze(
  Object.fromEntries(
    vendorOptions.map((vendor) => [vendor.value, getDefaultTypeForVendor(vendor.value)]),
  ),
) as Readonly<Record<Vendor, LibraryTypeValue>>;

export type BulkDownloadResult = Readonly<{
  succeeded: number;
  failed: number;
  skipped: number;
}>;

const EMPTY_BULK_RESULT: BulkDownloadResult = { succeeded: 0, failed: 0, skipped: 0 };
const BULK_DOWNLOAD_CONCURRENCY = 3;

export type LibrariesPageModel = ReturnType<typeof createLibrariesPageModel>;

type LibrariesPageModelOptions = Readonly<{
  cache?: LibraryPackagesCache;
}>;

export function createLibrariesPageModel(options: LibrariesPageModelOptions = {}) {
  const cache = options.cache ?? sharedLibraryPackagesCache;
  const initialCached = cache.get();
  let packages = $state<readonly LibraryPackageSummary[]>(initialCached?.packages ?? []);
  let catalogStatus = $state<LibraryPackagesOutput['catalog_status']>(
    initialCached?.catalog_status ?? 'active',
  );
  let hasLoaded = $state(initialCached !== null);
  let loading = $state(initialCached === null);
  let refreshing = $state(false);
  let errorMessage = $state<string | null>(null);
  const pendingActions = new SvelteMap<string, PackageAction>();
  let activeVendor = $state<Vendor>(DEFAULT_VENDOR);
  let activeType = $state<LibraryTypeValue>(DEFAULT_TYPE_BY_VENDOR[DEFAULT_VENDOR]);
  const lastTypeByVendor = $state<Record<Vendor, LibraryTypeValue>>({ ...DEFAULT_TYPE_BY_VENDOR });
  let bulkDownloading = $state(false);
  let bulkTotal = $state(0);
  let bulkCompleted = $state(0);
  let bulkTargetIds = $state<readonly string[]>([]);

  let active = false;
  let disposed = false;
  let startPromise: Promise<void> | null = null;
  let lastRequestedRefreshKey = 0;
  let refreshQueued = false;
  const loadRequests = createDisposableRequestChannel(() => disposed);

  const isBusy = $derived(loading || refreshing || pendingActions.size > 0 || bulkDownloading);
  const filteredPackages = $derived(filterPackageRows(packages, activeVendor, activeType));
  const showPackageDisplayName = $derived(shouldShowPackageDisplayName(filteredPackages));
  const emptyMessage = $derived(
    getEmptyMessage(loading, hasLoaded, errorMessage, filteredPackages.length),
  );
  const latestStablePackages = $derived(selectLatestStablePackages(packages));
  const latestStablePendingCount = $derived(
    latestStablePackages.filter((row) => row.local_state !== 'verified').length,
  );
  const bulkProgressValue = $derived.by(() => {
    if (!bulkDownloading) {
      return 0;
    }
    const inFlight = bulkTargetIds.filter((id) => pendingActions.get(id) === 'download');
    return bulkCompleted + sumDownloadFractions(inFlight);
  });

  function start(): Promise<void> {
    if (startPromise !== null) {
      return startPromise;
    }
    if (disposed) {
      return Promise.resolve();
    }
    active = true;
    startPromise = loadOnStart();
    return startPromise;
  }

  async function loadOnStart(): Promise<void> {
    // A refresh requested before start is satisfied by this first load.
    refreshQueued = false;
    const isWarmStart = hasLoaded;
    await loadLibraries({
      mode: isWarmStart ? 'refresh' : 'initial',
      failureContext: t(
        isWarmStart ? 'libraries.error.refreshFailed' : 'libraries.error.loadFailed',
      ),
    });
  }

  /** Re-reads the already activated catalog projection after a shell refresh. */
  async function refreshCatalog(): Promise<void> {
    if (!active) {
      return;
    }
    if (isBusy) {
      refreshQueued = true;
      return;
    }
    refreshQueued = false;
    await loadLibraries({ mode: 'refresh', failureContext: t('libraries.error.refreshFailed') });
  }

  /** Consumes each shell refresh generation at most once. */
  function requestCatalogRefresh(refreshKey: number): void {
    if (disposed || refreshKey <= lastRequestedRefreshKey) {
      return;
    }
    lastRequestedRefreshKey = refreshKey;

    if (startPromise === null) {
      refreshQueued = true;
      return;
    }
    void refreshCatalog();
  }

  async function loadLibraries(options: LoadLibrariesOptions): Promise<void> {
    const requestId = loadRequests.begin();
    const isInitialLoad = options.mode === 'initial';
    if (isInitialLoad) {
      loading = true;
    } else {
      refreshing = true;
    }
    errorMessage = null;

    try {
      const output = await listLibraryPackages();
      if (!isCurrentLoadRequest(requestId)) {
        return;
      }
      commitOutput(output);
    } catch (error) {
      if (!isCurrentLoadRequest(requestId)) {
        return;
      }
      if (isInitialLoad) {
        packages = [];
        hasLoaded = false;
      }
      setError(options.failureContext, error);
    } finally {
      if (isCurrentLoadRequest(requestId)) {
        loading = false;
        if (!isInitialLoad) {
          refreshing = false;
        }
      }
      scheduleQueuedRefresh();
    }
  }

  function handleVendorChange(value: unknown): void {
    if (typeof value !== 'string' || !isVendor(value)) {
      return;
    }
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

  async function handleDownload(packageId: string): Promise<boolean> {
    return runPackageAction({
      packageId,
      action: 'download',
      origin: 'user',
      failureContext: t('libraries.error.downloadFailed'),
      execute: () => downloadLibraryPackage(packageId),
    });
  }

  async function handleDelete(packageId: string): Promise<boolean> {
    const row = packages.find((candidate) => candidate.package_id === packageId);
    if (!row || !shouldDeleteLibraryPackage(row)) {
      return false;
    }
    return runPackageAction({
      packageId,
      action: 'delete',
      origin: 'user',
      failureContext: t('libraries.error.deleteFailed'),
      execute: () => deleteLibraryPackage(packageId),
    });
  }

  async function downloadAllLatest(): Promise<BulkDownloadResult> {
    if (!active || isBusy) {
      return EMPTY_BULK_RESULT;
    }
    const targets = latestStablePackages.filter((row) => row.local_state !== 'verified');
    if (targets.length === 0) {
      return EMPTY_BULK_RESULT;
    }

    bulkDownloading = true;
    bulkTotal = targets.length;
    bulkCompleted = 0;
    bulkTargetIds = targets.map((row) => row.package_id);
    errorMessage = null;

    let succeeded = 0;
    let failed = 0;
    let skipped = 0;
    try {
      await runWithConcurrency(targets, BULK_DOWNLOAD_CONCURRENCY, async (row) => {
        try {
          const ran = await runPackageAction({
            packageId: row.package_id,
            action: 'download',
            origin: 'bulk',
            failureContext: t('libraries.error.downloadFailed'),
            execute: () => downloadLibraryPackage(row.package_id),
            suppressErrorBanner: true,
          });
          if (ran) {
            succeeded += 1;
          } else {
            skipped += 1;
          }
        } catch {
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
      scheduleQueuedRefresh();
    }
    return { succeeded, failed, skipped };
  }

  /** Applies the mutation response directly so successful work stays successful. */
  async function runPackageAction(options: RunPackageActionOptions): Promise<boolean> {
    if (
      !active ||
      loading ||
      refreshing ||
      pendingActions.has(options.packageId) ||
      (bulkDownloading && options.origin === 'user')
    ) {
      return false;
    }

    pendingActions.set(options.packageId, options.action);
    if (options.action === 'download') {
      clearDownloadProgress([options.packageId]);
    }
    errorMessage = null;

    try {
      const mutation = await options.execute();
      if (isModelActive()) {
        applyPackageMutation(mutation);
      }
      return true;
    } catch (error) {
      if (isModelActive()) {
        if (options.suppressErrorBanner) {
          reportClientError('library_package_mutation', error);
        } else {
          setError(options.failureContext, error);
        }
      }
      throw error;
    } finally {
      pendingActions.delete(options.packageId);
      scheduleQueuedRefresh();
    }
  }

  function scheduleQueuedRefresh(): void {
    if (active && refreshQueued && !isBusy) {
      void refreshCatalog();
    }
  }

  function applyPackageMutation(mutation: LibraryPackageMutation): void {
    const previousIndex = packages.findIndex((row) => row.package_id === mutation.package_id);
    if (mutation.package === null) {
      commitPackages(packages.filter((_, index) => index !== previousIndex));
      return;
    }
    const replacement = mutation.package;
    if (previousIndex < 0) {
      commitPackages([...packages, replacement]);
      return;
    }
    commitPackages(packages.map((row, index) => (index === previousIndex ? replacement : row)));
  }

  function commitPackages(nextPackages: readonly LibraryPackageSummary[]): void {
    packages = nextPackages;
    cache.set({ packages: nextPackages, catalog_status: catalogStatus });
    hasLoaded = true;
  }

  function commitOutput(output: LibraryPackagesOutput): void {
    catalogStatus = output.catalog_status;
    packages = output.packages;
    cache.set(output);
    hasLoaded = true;
  }

  function dispose(): void {
    if (disposed) {
      return;
    }
    active = false;
    disposed = true;
    loadRequests.invalidate();
  }

  function getLastValidTypeForVendor(vendor: Vendor): LibraryTypeValue {
    const storedType = lastTypeByVendor[vendor];
    return isLibraryTypeForVendor(vendor, storedType)
      ? storedType
      : getDefaultTypeForVendor(vendor);
  }

  function isLibraryTypeForVendor(vendor: Vendor, value: string): value is LibraryTypeValue {
    return typeOptionsByVendor[vendor].some((option) => option.value === value);
  }

  function getEmptyMessage(
    isLoading: boolean,
    isAvailable: boolean,
    currentError: string | null,
    packageCount: number,
  ): string | null {
    if (isLoading) {
      return t('libraries.empty.loading');
    }
    if (!isAvailable && currentError !== null) {
      return t('libraries.empty.unavailable');
    }
    if (packageCount === 0) {
      return t('libraries.empty.none');
    }
    return null;
  }

  function isCurrentLoadRequest(requestId: number): boolean {
    return loadRequests.isActive(requestId) && !loadRequests.isDisposed();
  }

  function isModelActive(): boolean {
    return active && !disposed;
  }

  function setError(context: string, error: unknown): void {
    errorMessage = `${context}: ${formatPresentedError(error)}`;
    reportClientError('libraries_page_operation', error);
  }

  return {
    get packages() {
      return packages;
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
    get catalogStatus() {
      return catalogStatus;
    },
    get pendingActions() {
      return pendingActions as ReadonlyMap<string, PackageAction>;
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
    get bulkProgressValue() {
      return bulkProgressValue;
    },
    get isBusy() {
      return isBusy;
    },
    get filteredPackages(): LibraryPackageRow[] {
      return filteredPackages;
    },
    get showPackageDisplayName() {
      return showPackageDisplayName;
    },
    get emptyMessage() {
      return emptyMessage;
    },
    get latestStablePendingCount() {
      return latestStablePendingCount;
    },
    start,
    refreshCatalog,
    requestCatalogRefresh,
    handleVendorChange,
    handleTypeChange,
    handleDownload,
    handleDelete,
    downloadAllLatest,
    dispose,
  };
}
