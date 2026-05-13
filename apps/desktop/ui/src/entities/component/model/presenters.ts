import {
  formatCanonicalLibraryLabel,
  formatCompactLibraryLabel as formatSharedCompactLibraryLabel,
} from '@shared/graphics';

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
