import { canonicalizePackageVersion, comparePackageVersions } from '@shared/model';

/**
 * Presentation-version comparison for the game-details UI.
 *
 * Canonical package versions use the shared NuGet/SemVer precedence rules.
 * Invalid package identities fall back to backend-compatible PE FileVersion
 * ordering, preserving arbitrary technical-version segment counts.
 */

/** Semantic equality: trailing zero segments do not distinguish releases. */
export function versionsEqual(left: string, right: string): boolean {
  return compareVersionAsc(left, right) === 0;
}

/** Orders dotted version strings oldest-first, with a lexical fallback per segment. */
export function compareVersionAsc(left: string, right: string): number {
  if (canonicalizePackageVersion(left) !== null && canonicalizePackageVersion(right) !== null) {
    return comparePackageVersions(left, right);
  }
  return compareTechnicalVersionAsc(left, right);
}

function compareTechnicalVersionAsc(left: string, right: string): number {
  const leftParts = left.split('.');
  const rightParts = right.split('.');
  const length = Math.max(leftParts.length, rightParts.length);

  for (let index = 0; index < length; index += 1) {
    // Missing segments pad as 0 (trailing-zero-insensitive).
    const rawLeft = leftParts[index] ?? '0';
    const rawRight = rightParts[index] ?? '0';
    const compared = compareVersionSegment(rawLeft, rawRight);
    if (compared !== 0) {
      return compared;
    }
  }

  return 0;
}

/**
 * Compares arbitrary-length decimal segments without coercing them to JS
 * `Number`. Rust accepts `u64` segments, which can exceed Number's safe range;
 * compare normalized digit strings to preserve the backend ordering exactly.
 */
function compareVersionSegment(left: string, right: string): number {
  if (isDecimal(left) && isDecimal(right)) {
    const normalizedLeft = trimLeadingZeros(left);
    const normalizedRight = trimLeadingZeros(right);
    if (normalizedLeft.length !== normalizedRight.length) {
      return normalizedLeft.length < normalizedRight.length ? -1 : 1;
    }
    return compareCodeUnits(normalizedLeft, normalizedRight);
  }

  return compareCodeUnits(left, right);
}

/** Byte-order equivalent for ASCII version segments; avoids locale collation. */
function compareCodeUnits(left: string, right: string): number {
  if (left === right) {
    return 0;
  }
  return left < right ? -1 : 1;
}

function isDecimal(value: string): boolean {
  return /^\d+$/.test(value);
}

function trimLeadingZeros(value: string): string {
  const trimmed = value.replace(/^0+/, '');
  return trimmed === '' ? '0' : trimmed;
}

/** Orders dotted version strings newest-first. */
export function compareVersionDesc(left: string, right: string): number {
  return -compareVersionAsc(left, right);
}

/** Keeps the first spelling of each trailing-zero-equivalent version. */
export function collapseEquivalentVersions(versions: string[]): string[] {
  const kept: string[] = [];
  for (const version of versions) {
    if (!kept.some((existing) => versionsEqual(existing, version))) {
      kept.push(version);
    }
  }
  return kept;
}
