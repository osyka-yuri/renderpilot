import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { LibraryPackageState, LibraryPackageSummary } from '../model/types';

export async function listLibraryPackages(): Promise<LibraryPackageSummary[]> {
  return invokeDesktop<LibraryPackageSummary[]>('list_library_packages');
}

export async function downloadLibraryPackage(packageId: string): Promise<LibraryPackageState> {
  return invokeDesktop<LibraryPackageState>('download_library_package', {
    packageId: requireNonBlankString(packageId, 'packageId'),
  });
}

/**
 * Materializes a swap artifact by its id, downloading every member declared
 * by its catalog package, and returns the registered artifact ready to apply.
 */
export async function downloadArtifact(artifactId: string): Promise<LibraryPackageState> {
  return invokeDesktop<LibraryPackageState>('download_artifact', {
    artifactId: requireNonBlankString(artifactId, 'artifactId'),
  });
}

export async function deleteLibraryPackage(packageId: string): Promise<LibraryPackageState> {
  return invokeDesktop<LibraryPackageState>('delete_library_package', {
    packageId: requireNonBlankString(packageId, 'packageId'),
  });
}
