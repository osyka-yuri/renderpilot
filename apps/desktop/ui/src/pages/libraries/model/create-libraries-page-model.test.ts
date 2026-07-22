import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LibraryPackageState, LibraryPackageSummary, ReleaseChannel } from '@entities/library';
import type * as SharedLib from '@shared/lib';

const mocks = vi.hoisted(() => ({
  listLibraryPackages: vi.fn<() => Promise<LibraryPackageSummary[]>>(),
  downloadLibraryPackage: vi.fn<(packageId: string) => Promise<LibraryPackageState>>(),
  deleteLibraryPackage: vi.fn<(packageId: string) => Promise<LibraryPackageState>>(),
  clearDownloadProgress: vi.fn<(ids: readonly string[]) => void>(),
  sumDownloadFractions: vi.fn<(ids: readonly string[]) => number>(),
}));

vi.mock('@entities/library', () => ({
  listLibraryPackages: mocks.listLibraryPackages,
  downloadLibraryPackage: mocks.downloadLibraryPackage,
  deleteLibraryPackage: mocks.deleteLibraryPackage,
}));

vi.mock('@shared/lib', async (importOriginal) => {
  const actual = await importOriginal<typeof SharedLib>();
  return {
    ...actual,
    clearDownloadProgress: mocks.clearDownloadProgress,
    sumDownloadFractions: mocks.sumDownloadFractions,
  };
});

import { createLibrariesPageModel } from './create-libraries-page-model.svelte';
import { createLibraryPackagesCache, type LibraryPackagesCache } from './library-packages-cache';
import { packagesOf, type PackageFixture } from './library-package-test-fixtures';

function packageFixture(options: {
  id: string;
  lib: string;
  version: string;
  build?: ReleaseChannel;
  isDownloaded?: boolean;
}): PackageFixture {
  const isIntel = options.lib.startsWith('libxess');
  return {
    id: options.id,
    vendor: isIntel ? 'intel' : 'nvidia',
    technology: isIntel
      ? 'intel_xess'
      : options.lib === 'nvngx_dlss'
        ? 'dlss_super_resolution'
        : options.lib === 'nvngx_dlssg'
          ? 'dlss_frame_generation'
          : options.lib,
    variant: isIntel ? 'dx12_runtime' : 'runtime',
    version: options.version,
    channel: options.build ?? 'stable',
    isDownloaded: options.isDownloaded,
  };
}

function state(packageId: string, isDownloaded: boolean): LibraryPackageState {
  return {
    package_id: packageId,
    version: '',
    is_downloaded: isDownloaded,
    artifact_id: isDownloaded ? `catalog:${packageId}` : null,
  };
}

describe('createLibrariesPageModel', () => {
  let cache: LibraryPackagesCache;

  beforeEach(() => {
    vi.clearAllMocks();
    cache = createLibraryPackagesCache();
    mocks.downloadLibraryPackage.mockImplementation((id) => Promise.resolve(state(id, true)));
    mocks.deleteLibraryPackage.mockImplementation((id) => Promise.resolve(state(id, false)));
    mocks.clearDownloadProgress.mockReturnValue(undefined);
    mocks.sumDownloadFractions.mockReturnValue(0);
  });

  async function loadedModel(specs: PackageFixture[]) {
    mocks.listLibraryPackages.mockResolvedValue(packagesOf(specs));
    const model = createLibrariesPageModel({ cache });
    await model.start();
    return model;
  }

  it('uses cached packages immediately and refreshes them in the background on a new mount', async () => {
    const initialPackages = packagesOf([
      packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' }),
    ]);
    const refreshedPackages = packagesOf([
      packageFixture({ id: 'dlss-2', lib: 'nvngx_dlss', version: '2' }),
    ]);
    mocks.listLibraryPackages
      .mockResolvedValueOnce(initialPackages)
      .mockResolvedValueOnce(refreshedPackages);

    const firstModel = createLibrariesPageModel({ cache });
    await firstModel.start();

    const secondModel = createLibrariesPageModel({ cache });
    expect(secondModel.loading).toBe(false);
    expect(secondModel.filteredPackages.length).toBe(1);
    expect(secondModel.packages[0].package_id).toBe('dlss-1');

    const refresh = secondModel.start();
    expect(secondModel.refreshing).toBe(true);
    await refresh;

    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(2);
    expect(secondModel.packages[0].package_id).toBe('dlss-2');
  });

  it('coalesces a pre-mount refresh request with the mount load', async () => {
    mocks.listLibraryPackages.mockResolvedValue(
      packagesOf([packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' })]),
    );
    const model = createLibrariesPageModel({ cache });

    model.requestCatalogRefresh(1);
    await model.start();

    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);
  });

  it('shares one in-flight promise across repeated start calls', async () => {
    let resolvePackages!: (packages: LibraryPackageSummary[]) => void;
    mocks.listLibraryPackages.mockReturnValue(
      new Promise<LibraryPackageSummary[]>((resolve) => {
        resolvePackages = resolve;
      }),
    );
    const model = createLibrariesPageModel({ cache });

    const firstStart = model.start();
    const secondStart = model.start();

    expect(secondStart).toBe(firstStart);
    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);

    resolvePackages([]);
    await firstStart;
  });

  it('keeps cached packages available when a background refresh fails', async () => {
    const initialPackages = packagesOf([
      packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' }),
    ]);
    mocks.listLibraryPackages
      .mockResolvedValueOnce(initialPackages)
      .mockRejectedValueOnce(new Error('offline'));

    const firstModel = createLibrariesPageModel({ cache });
    await firstModel.start();
    const secondModel = createLibrariesPageModel({ cache });
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    try {
      await secondModel.start();

      expect(secondModel.packages).toEqual(initialPackages);
      expect(secondModel.errorMessage).not.toBeNull();
      expect(cache.get()).toEqual(initialPackages);
    } finally {
      consoleError.mockRestore();
    }
  });

  it('applies a successful mutation response without a fallible state refresh', async () => {
    const model = await loadedModel([
      packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' }),
    ]);

    await expect(model.handleDownload('dlss-1')).resolves.toBe(true);

    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);
    expect(model.packages[0].is_downloaded).toBe(true);
    expect(model.errorMessage).toBeNull();

    const nextModel = createLibrariesPageModel({ cache });
    expect(nextModel.packages[0].is_downloaded).toBe(true);
  });

  it('applies delete state directly to the matching row', async () => {
    const model = await loadedModel([
      packageFixture({
        id: 'dlss-1',
        lib: 'nvngx_dlss',
        version: '1',
        isDownloaded: true,
      }),
    ]);

    await expect(model.handleDelete('dlss-1')).resolves.toBe(true);
    expect(model.packages[0].is_downloaded).toBe(false);
  });

  it('downloads every pending latest package and reports the count', async () => {
    const model = await loadedModel([
      packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' }),
      packageFixture({ id: 'xess-1', lib: 'libxess', version: '1' }),
    ]);

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 2, failed: 0, skipped: 0 });
    expect(mocks.downloadLibraryPackage.mock.calls.map((call) => call[0]).sort()).toEqual([
      'dlss-1',
      'xess-1',
    ]);
    expect(model.bulkDownloading).toBe(false);
  });

  it('blocks user actions while bulk workers own the mutation queue', async () => {
    const model = await loadedModel([
      packageFixture({ id: 'a', lib: 'technology_a', version: '1' }),
      packageFixture({ id: 'b', lib: 'technology_b', version: '1' }),
      packageFixture({ id: 'c', lib: 'technology_c', version: '1' }),
      packageFixture({ id: 'd', lib: 'technology_d', version: '1' }),
    ]);
    const resolvers: Record<string, (value: LibraryPackageState) => void> = {};
    mocks.downloadLibraryPackage.mockImplementation(
      (id) =>
        new Promise<LibraryPackageState>((resolve) => {
          resolvers[id] = resolve;
        }),
    );

    const bulk = model.downloadAllLatest();

    expect(mocks.downloadLibraryPackage).toHaveBeenCalledTimes(3);
    await expect(model.handleDownload('d')).resolves.toBe(false);
    expect(mocks.downloadLibraryPackage.mock.calls.flat()).not.toContain('d');

    for (const id of ['a', 'b', 'c']) {
      resolvers[id](state(id, true));
    }
    await vi.waitFor(() => {
      expect(resolvers.d).toBeTypeOf('function');
    });
    resolvers.d(state('d', true));

    await expect(bulk).resolves.toEqual({ succeeded: 4, failed: 0, skipped: 0 });
    expect(mocks.downloadLibraryPackage.mock.calls.map(([id]) => id).sort()).toEqual([
      'a',
      'b',
      'c',
      'd',
    ]);
  });

  it('skips packages that are already downloaded', async () => {
    const model = await loadedModel([
      packageFixture({
        id: 'dlss-1',
        lib: 'nvngx_dlss',
        version: '1',
        isDownloaded: true,
      }),
      packageFixture({ id: 'xess-1', lib: 'libxess', version: '1' }),
    ]);

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 1, failed: 0, skipped: 0 });
    expect(mocks.downloadLibraryPackage).toHaveBeenCalledWith('xess-1');
  });

  it('counts a failed package without aborting the rest of the batch', async () => {
    mocks.downloadLibraryPackage.mockImplementation((id) =>
      id === 'xess-1' ? Promise.reject(new Error('boom')) : Promise.resolve(state(id, true)),
    );
    const model = await loadedModel([
      packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' }),
      packageFixture({ id: 'xess-1', lib: 'libxess', version: '1' }),
    ]);

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 1, failed: 1, skipped: 0 });
    expect(mocks.downloadLibraryPackage).toHaveBeenCalledTimes(2);
    expect(model.errorMessage).toBeNull();
  });

  it('returns zeros when every latest package is already downloaded', async () => {
    const model = await loadedModel([
      packageFixture({
        id: 'dlss-1',
        lib: 'nvngx_dlss',
        version: '1',
        isDownloaded: true,
      }),
    ]);

    await expect(model.downloadAllLatest()).resolves.toEqual({
      succeeded: 0,
      failed: 0,
      skipped: 0,
    });
    expect(mocks.downloadLibraryPackage).not.toHaveBeenCalled();
  });

  it('drops a package list that resolves after the model is disposed', async () => {
    let resolvePackages!: (packages: LibraryPackageSummary[]) => void;
    mocks.listLibraryPackages.mockReturnValue(
      new Promise<LibraryPackageSummary[]>((resolve) => {
        resolvePackages = resolve;
      }),
    );
    const model = createLibrariesPageModel({ cache });
    const loading = model.start();

    model.dispose();
    resolvePackages(
      packagesOf([packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' })]),
    );
    await loading;

    expect(model.packages).toEqual([]);
    expect(cache.get()).toBeNull();
  });

  it('treats dispose as terminal and does not restart loading', async () => {
    const model = createLibrariesPageModel({ cache });

    model.dispose();
    await model.start();
    await expect(model.handleDownload('ignored')).resolves.toBe(false);
    await expect(model.downloadAllLatest()).resolves.toEqual({
      succeeded: 0,
      failed: 0,
      skipped: 0,
    });

    expect(mocks.listLibraryPackages).not.toHaveBeenCalled();
    expect(mocks.downloadLibraryPackage).not.toHaveBeenCalled();
  });

  it('does not let an older package request overwrite a newer result', async () => {
    let resolveFirst!: (packages: LibraryPackageSummary[]) => void;
    mocks.listLibraryPackages
      .mockReturnValueOnce(
        new Promise<LibraryPackageSummary[]>((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockResolvedValueOnce(
        packagesOf([packageFixture({ id: 'new', lib: 'nvngx_dlss', version: '2' })]),
      );
    const model = createLibrariesPageModel({ cache });

    const older = model.start();
    model.requestCatalogRefresh(1);
    resolveFirst(packagesOf([packageFixture({ id: 'old', lib: 'nvngx_dlss', version: '1' })]));
    await older;
    await vi.waitFor(() => {
      expect(model.packages.map((row) => row.package_id)).toEqual(['new']);
    });
  });

  it('aggregates finished packages and in-flight byte fractions', async () => {
    const model = await loadedModel([
      packageFixture({ id: 'a', lib: 'nvngx_dlss', version: '1' }),
      packageFixture({ id: 'b', lib: 'nvngx_dlssg', version: '1' }),
    ]);
    const resolvers: Record<string, (value: LibraryPackageState) => void> = {};
    mocks.downloadLibraryPackage.mockImplementation(
      (id) =>
        new Promise<LibraryPackageState>((resolve) => {
          resolvers[id] = resolve;
        }),
    );
    mocks.sumDownloadFractions.mockReturnValue(0.5);

    const done = model.downloadAllLatest();

    expect(model.bulkDownloading).toBe(true);
    expect(model.bulkTotal).toBe(2);
    expect(model.bulkProgressValue).toBeCloseTo(0.5);
    expect(mocks.sumDownloadFractions).toHaveBeenCalledWith(['a', 'b']);

    resolvers.a(state('a', true));
    resolvers.b(state('b', true));
    await done;
    expect(model.bulkProgressValue).toBe(0);
  });

  it('re-reads the compact package projection on refresh', async () => {
    const specs = [packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' })];
    const model = await loadedModel(specs);
    vi.clearAllMocks();
    mocks.listLibraryPackages.mockResolvedValue(packagesOf(specs));

    await model.refreshCatalog();

    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);
  });

  it('derives package-name visibility from the active filtered list', async () => {
    const model = await loadedModel([
      {
        id: 'dlss-old',
        vendor: 'nvidia',
        technology: 'dlss_super_resolution',
        variant: 'runtime',
        displayName: 'NVIDIA DLSS Super Resolution',
      },
      {
        id: 'dlss-new',
        vendor: 'nvidia',
        technology: 'dlss_super_resolution',
        variant: 'runtime',
        displayName: 'NVIDIA DLSS Super Resolution',
      },
      {
        id: 'fsr-runtime',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        displayName: 'AMD FidelityFX Super Resolution',
      },
      {
        id: 'fsr-sdk',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'sdk_bundle',
        displayName: 'AMD FidelityFX SDK DirectX 12',
      },
    ]);

    expect(model.showPackageDisplayName).toBe(false);

    model.handleVendorChange('amd');

    expect(model.activeType).toBe('fsr');
    expect(model.showPackageDisplayName).toBe(true);
  });

  it('deduplicates refresh generations and runs a queued refresh after a mutation', async () => {
    const specs = [packageFixture({ id: 'dlss-1', lib: 'nvngx_dlss', version: '1' })];
    const model = await loadedModel(specs);
    let resolveDownload!: (value: LibraryPackageState) => void;
    mocks.downloadLibraryPackage.mockReturnValue(
      new Promise<LibraryPackageState>((resolve) => {
        resolveDownload = resolve;
      }),
    );
    mocks.listLibraryPackages.mockClear();
    mocks.listLibraryPackages.mockResolvedValue(packagesOf(specs));

    const download = model.handleDownload('dlss-1');
    model.requestCatalogRefresh(1);
    model.requestCatalogRefresh(1);
    expect(mocks.listLibraryPackages).not.toHaveBeenCalled();

    resolveDownload(state('dlss-1', true));
    await download;
    await vi.waitFor(() => {
      expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);
    });

    model.requestCatalogRefresh(1);
    await Promise.resolve();
    expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(1);

    model.requestCatalogRefresh(2);
    await vi.waitFor(() => {
      expect(mocks.listLibraryPackages).toHaveBeenCalledTimes(2);
    });
  });
});
