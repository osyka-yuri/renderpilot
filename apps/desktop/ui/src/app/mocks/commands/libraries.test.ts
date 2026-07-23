import type { LibraryPackageState, LibraryPackageSummary } from '@entities/library';
import { clearPreviewInvoker, invokePreviewCommand } from '@shared/api-preview';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { mockInvoker, registerMockInvoker, resetMockDesktopState } from '../desktop';

async function listPackages(): Promise<LibraryPackageSummary[]> {
  return mockInvoker<LibraryPackageSummary[]>('list_library_packages');
}

describe('preview library commands', () => {
  beforeEach(() => {
    clearPreviewInvoker();
    resetMockDesktopState();
  });

  afterEach(() => {
    clearPreviewInvoker();
  });

  it('registers library commands with the preview transport', async () => {
    registerMockInvoker();

    await expect(
      invokePreviewCommand<LibraryPackageSummary[]>('list_library_packages'),
    ).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ package_id: 'preview:nvidia:dlss:3.10.0' }),
      ]),
    );
  });

  it('lists seeded packages and persists a package download', async () => {
    const packages = await listPackages();
    const pending = packages.find((item) => !item.is_downloaded);

    expect(pending).toBeDefined();
    if (!pending) {
      throw new Error('Expected a pending preview library package.');
    }

    const downloaded = await mockInvoker<LibraryPackageState>('download_library_package', {
      packageId: pending.package_id,
    });

    expect(downloaded).toEqual({
      package_id: pending.package_id,
      version: pending.release.version,
      is_downloaded: true,
      artifact_id: pending.artifact_id,
    });
    expect((await listPackages()).find((item) => item.package_id === pending.package_id)).toEqual(
      expect.objectContaining({ is_downloaded: true }),
    );
  });

  it('deletes a downloaded package and supports artifact downloads', async () => {
    const downloaded = (await listPackages()).find((item) => item.is_downloaded);

    expect(downloaded).toBeDefined();
    if (!downloaded) {
      throw new Error('Expected a downloaded preview library package.');
    }

    const deleted = await mockInvoker<LibraryPackageState>('delete_library_package', {
      packageId: downloaded.package_id,
    });
    expect(deleted.is_downloaded).toBe(false);
    expect(deleted.artifact_id).toBeNull();

    const restored = await mockInvoker<LibraryPackageState>('download_artifact', {
      artifactId: downloaded.artifact_id,
    });
    expect(restored.is_downloaded).toBe(true);
    expect(restored.artifact_id).toBe(downloaded.artifact_id);
  });

  it('rejects an unknown package id', async () => {
    await expect(
      mockInvoker<LibraryPackageState>('download_library_package', {
        packageId: 'missing:package',
      }),
    ).rejects.toThrow('could not find library package missing:package');
  });
});
