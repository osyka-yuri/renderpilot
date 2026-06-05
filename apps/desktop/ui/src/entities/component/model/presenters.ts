import {
  displayLibraryFilePath,
  formatCanonicalLibraryLabel,
  formatCompactLibraryLabel as formatSharedCompactLibraryLabel,
} from '@shared/graphics';

import type { GraphicsComponent } from './types';

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
  component: Pick<GraphicsComponent, 'technology' | 'files'>,
): string | null {
  return displayLibraryFilePath(component.technology, component.files);
}
