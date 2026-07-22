import type { LibraryPackageSummary } from '@entities/library';

export type LibraryPackagesCache = Readonly<{
  get(): readonly LibraryPackageSummary[] | null;
  set(packages: readonly LibraryPackageSummary[]): void;
}>;

export function createLibraryPackagesCache(): LibraryPackagesCache {
  let packages: readonly LibraryPackageSummary[] | null = null;

  return {
    get: () => packages,
    set: (nextPackages) => {
      packages = nextPackages;
    },
  };
}

export const sharedLibraryPackagesCache = createLibraryPackagesCache();
