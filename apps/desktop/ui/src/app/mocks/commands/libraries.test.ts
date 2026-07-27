import type {
  LibraryPackageMutation,
  LibraryPackageState,
  LibraryPackagesOutput,
} from '@entities/library';
import { clearPreviewInvoker, invokePreviewCommand } from '@shared/api-preview';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { mockInvoker, registerMockInvoker, resetMockDesktopState } from '../desktop';

async function listPackages(): Promise<LibraryPackagesOutput['packages']> {
  return (await mockInvoker<LibraryPackagesOutput>('list_library_packages')).packages;
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

    const output = await invokePreviewCommand<LibraryPackagesOutput>('list_library_packages');

    expect(output.catalog_status).toBe('active');
    expect(
      output.packages.some(({ package_id }) => package_id === 'preview:nvidia:dlss:3.10.0'),
    ).toBe(true);
  });

  it('lists seeded packages and persists a package download', async () => {
    const packages = await listPackages();
    const pending = packages.find((item) => item.local_state !== 'verified');

    expect(pending).toBeDefined();
    if (!pending) {
      throw new Error('Expected a pending preview library package.');
    }

    const downloaded = await mockInvoker<LibraryPackageMutation>('download_library_package', {
      packageId: pending.package_id,
    });

    expect(downloaded.package).toEqual(
      expect.objectContaining({
        package_id: pending.package_id,
        local_state: 'verified',
      }),
    );
    expect((await listPackages()).find((item) => item.package_id === pending.package_id)).toEqual(
      expect.objectContaining({ local_state: 'verified' }),
    );
  });

  it('deletes a downloaded package and materializes a catalog candidate', async () => {
    const downloaded = (await listPackages()).find((item) => item.local_state === 'verified');

    expect(downloaded).toBeDefined();
    if (!downloaded) {
      throw new Error('Expected a downloaded preview library package.');
    }

    const deleted = await mockInvoker<LibraryPackageMutation>('delete_library_package', {
      packageId: downloaded.package_id,
    });
    expect(deleted.package?.local_state).toBe('absent');

    const restored = await mockInvoker<LibraryPackageState>('download_artifact', {
      artifactId: 'artifact:dlss:3.7.20',
    });
    expect(restored.is_downloaded).toBe(true);
    expect(restored.package_id).toBe('nvidia-dlss-3.7.20');
    expect(restored.artifact_id).toBe('artifact:dlss:3.7.20');
  });

  it('rejects an unknown package id', async () => {
    await expect(
      mockInvoker<LibraryPackageState>('download_library_package', {
        packageId: 'missing:package',
      }),
    ).rejects.toThrow('could not find library package missing:package');
  });
});
