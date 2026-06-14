import { describe, expect, it, vi, beforeEach } from 'vitest';

import type {
  BuildType,
  LibraryManifest,
  LibraryManifestEntry,
  LibraryState,
} from '@entities/library';

const mocks = vi.hoisted(() => ({
  getLibrariesManifest: vi.fn<() => Promise<LibraryManifest>>(),
  fetchLibrariesManifest: vi.fn<() => Promise<LibraryManifest>>(),
  getLibraryStates: vi.fn<() => Promise<LibraryState[]>>(),
  downloadLibrary: vi.fn<(entryId: string) => Promise<LibraryState>>(),
  deleteLibrary: vi.fn<(entryId: string) => Promise<LibraryState>>(),
  clearDownloadProgress: vi.fn<(ids: readonly string[]) => void>(),
  sumDownloadFractions: vi.fn<(ids: readonly string[]) => number>(),
}));

vi.mock('@entities/library', () => mocks);

import { createLibrariesPageModel } from './create-libraries-page-model.svelte';

function entry(options: {
  id: string;
  lib: string;
  sort: string;
  build?: BuildType;
}): LibraryManifestEntry {
  return {
    entry_id: options.id,
    library: { id: options.lib, file_name: `${options.lib}.dll` },
    version: { value: options.sort, sort_key: options.sort },
    build: { type: options.build ?? 'stable', label: null },
    files: {
      dll: { size_bytes: 1, hashes: { sha256: '0'.repeat(64) } },
      zip: { size_bytes: 1, download_url: 'https://example.com/file.zip' },
    },
    signature: { status: 'unsigned' },
  };
}

function manifestOf(entries: LibraryManifestEntry[]): LibraryManifest {
  return { schema_version: 1, generated_at: '2024-01-01T00:00:00Z', entries };
}

function state(id: string, isDownloaded: boolean): LibraryState {
  return { id, version: '', is_downloaded: isDownloaded, local_path: null, artifact_id: null };
}

describe('createLibrariesPageModel.downloadAllLatest', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.downloadLibrary.mockResolvedValue(state('x', true));
    mocks.clearDownloadProgress.mockReturnValue(undefined);
    mocks.sumDownloadFractions.mockReturnValue(0);
  });

  async function loadedModel(entries: LibraryManifestEntry[], states: LibraryState[] = []) {
    mocks.getLibrariesManifest.mockResolvedValue(manifestOf(entries));
    mocks.getLibraryStates.mockResolvedValue(states);

    const model = createLibrariesPageModel();
    model.init();
    await model.loadInitialLibraries();
    return model;
  }

  it('downloads every pending latest entry and reports the count', async () => {
    const model = await loadedModel([
      entry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' }),
      entry({ id: 'xess-1', lib: 'libxess', sort: '001' }),
    ]);

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 2, failed: 0, skipped: 0 });
    expect(mocks.downloadLibrary).toHaveBeenCalledTimes(2);
    expect(mocks.downloadLibrary.mock.calls.map((c) => c[0]).sort()).toEqual(['dlss-1', 'xess-1']);
    expect(model.bulkDownloading).toBe(false);
  });

  it('skips entries that are already downloaded', async () => {
    const model = await loadedModel(
      [
        entry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' }),
        entry({ id: 'xess-1', lib: 'libxess', sort: '001' }),
      ],
      [state('dlss-1', true)],
    );

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 1, failed: 0, skipped: 0 });
    expect(mocks.downloadLibrary).toHaveBeenCalledTimes(1);
    expect(mocks.downloadLibrary).toHaveBeenCalledWith('xess-1');
  });

  it('counts a failed entry without aborting the rest of the batch', async () => {
    mocks.downloadLibrary.mockImplementation((id: string) =>
      id === 'xess-1' ? Promise.reject(new Error('boom')) : Promise.resolve(state(id, true)),
    );

    const model = await loadedModel([
      entry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' }),
      entry({ id: 'xess-1', lib: 'libxess', sort: '001' }),
    ]);

    const result = await model.downloadAllLatest();

    expect(result.succeeded).toBe(1);
    expect(result.failed).toBe(1);
    expect(mocks.downloadLibrary).toHaveBeenCalledTimes(2);
    expect(model.bulkDownloading).toBe(false);
    // Bulk failures are reported via a single summary toast, not the page-level
    // error banner.
    expect(model.errorMessage).toBeNull();
  });

  it('returns zeros and downloads nothing when everything is up to date', async () => {
    const model = await loadedModel(
      [entry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' })],
      [state('dlss-1', true)],
    );

    const result = await model.downloadAllLatest();

    expect(result).toEqual({ succeeded: 0, failed: 0, skipped: 0 });
    expect(mocks.downloadLibrary).not.toHaveBeenCalled();
  });

  it('drops a manifest load that resolves after the model is disposed', async () => {
    let resolveManifest!: (manifest: LibraryManifest) => void;
    mocks.getLibrariesManifest.mockReturnValue(
      new Promise<LibraryManifest>((resolve) => {
        resolveManifest = resolve;
      }),
    );
    mocks.getLibraryStates.mockResolvedValue([]);

    const model = createLibrariesPageModel();
    model.init();
    const loading = model.loadInitialLibraries();

    // Tear the page down while the load is still in flight, then let it resolve.
    model.dispose();
    resolveManifest(manifestOf([entry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' })]));
    await loading;

    // The stale result must not be applied to a disposed model.
    expect(model.manifest).toBeNull();
  });

  it('aggregates progress as finished count plus in-flight byte fractions', async () => {
    const model = await loadedModel(
      [entry({ id: 'a', lib: 'a', sort: '001' }), entry({ id: 'b', lib: 'b', sort: '001' })],
      [state('a', false), state('b', false)],
    );

    // Hold both downloads in-flight so progress can be sampled mid-batch. The
    // pool starts every worker synchronously up to the first `await`, so the
    // in-flight state is observable right after the call without any ticks.
    const resolvers: Record<string, (s: LibraryState) => void> = {};
    mocks.downloadLibrary.mockImplementation(
      (id) =>
        new Promise<LibraryState>((resolve) => {
          resolvers[id] = resolve;
        }),
    );
    mocks.sumDownloadFractions.mockReturnValue(0.5);

    const done = model.downloadAllLatest();

    // Both in-flight, none finished yet: 0 completed + the mocked 0.5 fraction.
    expect(model.bulkDownloading).toBe(true);
    expect(model.bulkTotal).toBe(2);
    expect(model.bulkProgressValue).toBeCloseTo(0.5);
    expect(mocks.sumDownloadFractions).toHaveBeenCalledWith(['a', 'b']);

    resolvers.a(state('a', true));
    resolvers.b(state('b', true));
    await done;

    // The bar is hidden once the batch ends.
    expect(model.bulkDownloading).toBe(false);
    expect(model.bulkProgressValue).toBe(0);
  });
});
