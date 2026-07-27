import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type {
  LibraryPackageMutation,
  LibraryPackageState,
  LibraryPackagesOutput,
} from '../model/types';

export async function listLibraryPackages(): Promise<LibraryPackagesOutput> {
  return invokeDesktop<LibraryPackagesOutput>('list_library_packages');
}

export async function downloadLibraryPackage(packageId: string): Promise<LibraryPackageMutation> {
  return invokeDesktop<LibraryPackageMutation>('download_library_package', {
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

export async function deleteLibraryPackage(packageId: string): Promise<LibraryPackageMutation> {
  return invokeDesktop<LibraryPackageMutation>('delete_library_package', {
    packageId: requireNonBlankString(packageId, 'packageId'),
  });
}
