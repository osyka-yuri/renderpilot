import type { LibraryPackagesOutput } from '@entities/library';

export type LibraryPackagesCache = Readonly<{
  get(): LibraryPackagesOutput | null;
  set(output: LibraryPackagesOutput): void;
}>;

export function createLibraryPackagesCache(): LibraryPackagesCache {
  let output: LibraryPackagesOutput | null = null;

  return {
    get: () => output,
    set: (nextOutput) => {
      output = nextOutput;
    },
  };
}

export const sharedLibraryPackagesCache = createLibraryPackagesCache();
