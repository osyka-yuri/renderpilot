import type { LibraryPackageState, LibraryPackageSummary } from '@entities/library';

import { mockState } from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockListLibraryPackages(): Promise<LibraryPackageSummary[]> {
  return resolveMock(() => clone(mockState.libraryPackages));
}

export function mockDownloadLibraryPackage(packageId: string): Promise<LibraryPackageState> {
  return setLibraryPackageDownloaded(packageId, true, 'packageId');
}

export function mockDeleteLibraryPackage(packageId: string): Promise<LibraryPackageState> {
  return setLibraryPackageDownloaded(packageId, false, 'packageId');
}

export function mockDownloadArtifact(artifactId: string): Promise<LibraryPackageState> {
  return resolveMock(() => {
    const normalizedArtifactId = requireNonEmptyText(artifactId, 'artifactId');
    const packageIndex = mockState.libraryPackages.findIndex(
      (item) => item.artifact_id === normalizedArtifactId,
    );

    if (packageIndex < 0) {
      throw new Error(`Mock preview could not find library artifact ${normalizedArtifactId}.`);
    }

    return updateLibraryPackage(packageIndex, true);
  });
}

function setLibraryPackageDownloaded(
  packageId: string,
  isDownloaded: boolean,
  label: string,
): Promise<LibraryPackageState> {
  return resolveMock(() => {
    const normalizedPackageId = requireNonEmptyText(packageId, label);
    const packageIndex = mockState.libraryPackages.findIndex(
      (item) => item.package_id === normalizedPackageId,
    );

    if (packageIndex < 0) {
      throw new Error(`Mock preview could not find library package ${normalizedPackageId}.`);
    }

    return updateLibraryPackage(packageIndex, isDownloaded);
  });
}

function updateLibraryPackage(packageIndex: number, isDownloaded: boolean): LibraryPackageState {
  const current = mockState.libraryPackages[packageIndex];
  const next = { ...current, is_downloaded: isDownloaded };
  mockState.libraryPackages = mockState.libraryPackages.map((item, index) =>
    index === packageIndex ? next : item,
  );

  return clone({
    package_id: next.package_id,
    version: next.release.version,
    is_downloaded: next.is_downloaded,
    artifact_id: next.is_downloaded ? next.artifact_id : null,
  });
}
