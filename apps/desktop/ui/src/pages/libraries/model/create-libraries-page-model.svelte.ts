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
} from './libraries-page-model';
import { describeCommandError } from '@shared/api';
import { runWithConcurrency } from '@shared/concurrency';
import { clearDownloadProgress, sumDownloadFractions } from '@shared/lib';
import { createDisposableRequestChannel } from '@shared/requests';
import { t } from '@shared/i18n';
import {
  type LibraryPackageState,
  type LibraryPackageSummary,
  listLibraryPackages,
  downloadLibraryPackage,
  deleteLibraryPackage,
} from '@entities/library';

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
  execute: (packageId: string) => Promise<LibraryPackageState>;
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

export function createLibrariesPageModel() {
  let packages = $state<LibraryPackageSummary[]>([]);
  let hasLoaded = $state(false);
  let loading = $state(true);
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

  let mounted = false;
  let lastRequestedRefreshKey = 0;
  let refreshQueued = false;
  const loadRequests = createDisposableRequestChannel(() => !mounted);

  const isBusy = $derived(loading || refreshing || pendingActions.size > 0 || bulkDownloading);
  const filteredPackages = $derived(filterPackageRows(packages, activeVendor, activeType));
  const showPackageDisplayName = $derived(shouldShowPackageDisplayName(filteredPackages));
  const emptyMessage = $derived(
    getEmptyMessage(loading, hasLoaded, errorMessage, filteredPackages.length),
  );
  const latestStablePackages = $derived(selectLatestStablePackages(packages));
  const latestStablePendingCount = $derived(
    latestStablePackages.filter((row) => !row.is_downloaded).length,
  );
  const bulkProgressValue = $derived.by(() => {
    if (!bulkDownloading) {
      return 0;
    }
    const inFlight = bulkTargetIds.filter((id) => pendingActions.get(id) === 'download');
    return bulkCompleted + sumDownloadFractions(inFlight);
  });

  async function loadInitialLibraries(): Promise<void> {
    await loadLibraries({ mode: 'initial', failureContext: t('libraries.error.loadFailed') });
  }

  /** Re-reads the already activated catalog projection after a shell refresh. */
  async function refreshCatalog(): Promise<void> {
    if (isBusy) {
      refreshQueued = true;
      return;
    }
    refreshQueued = false;
    await loadLibraries({ mode: 'refresh', failureContext: t('libraries.error.refreshFailed') });
  }

  /** Consumes each shell refresh generation at most once. */
  function requestCatalogRefresh(refreshKey: number): void {
    if (refreshKey <= lastRequestedRefreshKey) {
      return;
    }
    lastRequestedRefreshKey = refreshKey;
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
      const nextPackages = await listLibraryPackages();
      if (!isCurrentLoadRequest(requestId)) {
        return;
      }
      packages = nextPackages;
      hasLoaded = true;
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
      execute: downloadLibraryPackage,
    });
  }

  async function handleDelete(packageId: string): Promise<boolean> {
    return runPackageAction({
      packageId,
      action: 'delete',
      origin: 'user',
      failureContext: t('libraries.error.deleteFailed'),
      execute: deleteLibraryPackage,
    });
  }

  async function downloadAllLatest(): Promise<BulkDownloadResult> {
    if (isBusy) {
      return EMPTY_BULK_RESULT;
    }
    const targets = latestStablePackages.filter((row) => !row.is_downloaded);
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
            execute: downloadLibraryPackage,
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
      const state = await options.execute(options.packageId);
      if (mounted) {
        applyPackageState(state);
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
      pendingActions.delete(options.packageId);
      scheduleQueuedRefresh();
    }
  }

  function scheduleQueuedRefresh(): void {
    if (mounted && refreshQueued && !isBusy) {
      void refreshCatalog();
    }
  }

  function applyPackageState(state: LibraryPackageState): void {
    packages = packages.map((row) =>
      row.package_id === state.package_id ? { ...row, is_downloaded: state.is_downloaded } : row,
    );
  }

  function init(): void {
    mounted = true;
    scheduleQueuedRefresh();
  }

  function dispose(): void {
    mounted = false;
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

  function setError(context: string, error: unknown): void {
    errorMessage = `${context}: ${describeCommandError(error)}`;
    console.error(`${context}:`, error);
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
    loadInitialLibraries,
    refreshCatalog,
    requestCatalogRefresh,
    handleVendorChange,
    handleTypeChange,
    handleDownload,
    handleDelete,
    downloadAllLatest,
    init,
    dispose,
  };
}
