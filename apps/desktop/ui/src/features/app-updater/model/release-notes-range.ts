import { findReleaseHeadings, type VersionedReleaseHeading } from '@shared/model';

/**
 * Select the newest-first changelog interval represented by an update offer.
 *
 * Official manifests are validated at publication time: their notes start at
 * the offered version and contain unique release headings in descending order.
 * The installed version is therefore an exclusive boundary, not a value that
 * the UI must independently order with another SemVer implementation.
 *
 * Unversioned or legacy notes are returned unchanged so non-history manifests
 * retain the existing presentation behavior.
 */
export function selectUpdateReleaseNotes(
  input: string,
  currentVersion: string,
  offeredVersion: string,
): string {
  const text = input.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
  if (text.length === 0) {
    return '';
  }

  const headings = findReleaseHeadings(text).filter(
    (heading): heading is VersionedReleaseHeading => heading.kind === 'versioned',
  );
  const offeredIndex = headings.findIndex((heading) => heading.version === offeredVersion);
  if (offeredIndex < 0) {
    return text;
  }

  const currentIndex = headings.findIndex(
    (heading, index) => index >= offeredIndex && heading.version === currentVersion,
  );
  const end = currentIndex < 0 ? text.length : headings[currentIndex].start;
  return text.slice(headings[offeredIndex].start, end).trim();
}
