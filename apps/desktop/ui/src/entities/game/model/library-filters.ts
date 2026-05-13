import type { GameSummary } from './types';
import { normalizeUniqueTrimmedStrings } from '@shared/text';

export function normalizeLibraryValues(values: readonly string[]): string[] {
  return normalizeUniqueTrimmedStrings(values);
}

export function extractAvailableLibrariesFromCards(cards: readonly GameSummary[]): string[] {
  const libraries: string[] = [];

  for (const card of cards) {
    libraries.push(...card.library_tags);
  }

  return normalizeLibraryValues(libraries).filter(
    (library) => library.toLowerCase() !== 'unknown',
  );
}

/** Keep only values still present in the catalog union. */
export function intersectLibraries(
  selection: readonly string[],
  available: readonly string[],
): string[] {
  return intersectNormalizedLibraries(
    normalizeLibraryValues(selection),
    normalizeLibraryValues(available),
  );
}

export function hasPartialLibrarySelection(
  selectedLibraries: readonly string[],
  availableLibraryValues: readonly string[],
): boolean {
  const availableLibraries = normalizeLibraryValues(availableLibraryValues);

  if (availableLibraries.length === 0) {
    return false;
  }

  const selectedAvailableLibraries = intersectNormalizedLibraries(
    normalizeLibraryValues(selectedLibraries),
    availableLibraries,
  );

  return selectedAvailableLibraries.length < availableLibraries.length;
}

function intersectNormalizedLibraries(
  selection: readonly string[],
  available: readonly string[],
): string[] {
  if (available.length === 0) {
    return [];
  }

  const allowedLibraries = new Set(available);

  return selection.filter((library) => allowedLibraries.has(library));
}
