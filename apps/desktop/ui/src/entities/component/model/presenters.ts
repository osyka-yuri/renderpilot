import {
  displayLibraryFilePath,
  formatCanonicalLibraryLabel,
  formatCompactLibraryLabel as formatSharedCompactLibraryLabel,
  presentLibraryFiles,
} from '@shared/graphics';
import type { PresentedLibraryFiles } from '@shared/graphics';

import type { LibraryComponent } from './types';

export function formatComponentLabel(value?: string | null): string {
  return formatCanonicalLibraryLabel(value);
}

export function formatLabel(value?: string | null): string {
  return formatComponentLabel(value);
}

export function formatLibrary(value?: string | null): string {
  return formatComponentLabel(value);
}

export function formatCompactLibraryLabel(value?: string | null): string {
  return formatSharedCompactLibraryLabel(value);
}

export function displayComponentFilePath(
  component: Pick<LibraryComponent, 'technology' | 'files'>,
): string | null {
  return displayLibraryFilePath(component.technology, component.files);
}

export function presentComponentFiles(
  component: Pick<LibraryComponent, 'technology' | 'files'>,
): PresentedLibraryFiles | null {
  return presentLibraryFiles(component.technology, component.files);
}
