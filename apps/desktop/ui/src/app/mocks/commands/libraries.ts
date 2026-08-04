import type {
  LibraryPackageMutation,
  LibraryPackageState,
  LibraryPackagesOutput,
} from '@entities/library';

import { mockState } from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockListLibraryPackages(): Promise<LibraryPackagesOutput> {
  return resolveMock(() =>
    clone({ packages: mockState.libraryPackages, catalog_status: 'active' as const }),
  );
}

export function mockDownloadLibraryPackage(packageId: string): Promise<LibraryPackageMutation> {
  return resolveMock(() => {
    const packageIndex = requirePackageIndex(packageId, 'packageId');
    const next = updateLibraryPackage(packageIndex, true);
    return clone({ package_id: next.package_id, package: next });
  });
}

export function mockDeleteLibraryPackage(packageId: string): Promise<LibraryPackageMutation> {
  return resolveMock(() => {
    const packageIndex = requirePackageIndex(packageId, 'packageId');
    const current = mockState.libraryPackages[packageIndex];
    if (current.availability === 'local_only') {
      mockState.libraryPackages = mockState.libraryPackages.toSpliced(packageIndex, 1);
      return clone({ package_id: current.package_id, package: null });
    }
    const next = updateLibraryPackage(packageIndex, false);
    return clone({ package_id: next.package_id, package: next });
  });
}

export function mockDownloadArtifact(artifactId: string): Promise<LibraryPackageState> {
  return resolveMock(() => {
    const normalizedArtifactId = requireNonEmptyText(artifactId, 'artifactId');
    const candidate = mockState.detailsByGameId
      .values()
      .flatMap((details) => details.candidate_groups)
      .flatMap((group) => group.candidates)
      .find((item) => item.artifact_id === normalizedArtifactId);
    const catalogPackage = candidate?.catalog_package;
    if (!catalogPackage) {
      throw new Error(`Mock preview could not find library artifact ${normalizedArtifactId}.`);
    }

    const packageIndex = mockState.libraryPackages.findIndex(
      (item) => item.package_id === catalogPackage.package_id,
    );
    if (packageIndex >= 0) {
      updateLibraryPackage(packageIndex, true);
    }
    return clone({
      package_id: catalogPackage.package_id,
      version: catalogPackage.release.version,
      is_downloaded: true,
      artifact_id: normalizedArtifactId,
    });
  });
}

function requirePackageIndex(packageId: string, label: string): number {
  const normalizedPackageId = requireNonEmptyText(packageId, label);
  const packageIndex = mockState.libraryPackages.findIndex(
    (item) => item.package_id === normalizedPackageId,
  );

  if (packageIndex < 0) {
    throw new Error(`Mock preview could not find library package ${normalizedPackageId}.`);
  }
  return packageIndex;
}

function updateLibraryPackage(
  packageIndex: number,
  isDownloaded: boolean,
): (typeof mockState.libraryPackages)[number] {
  const current = mockState.libraryPackages[packageIndex];
  const next = {
    ...current,
    local_state: isDownloaded ? ('verified' as const) : ('absent' as const),
  };
  mockState.libraryPackages = mockState.libraryPackages.with(packageIndex, next);

  return next;
}
