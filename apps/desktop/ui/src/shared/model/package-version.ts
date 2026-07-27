/** Orders NuGet/SemVer2 package identities without converting numeric segments to Number. */
const MAX_U64 = 18_446_744_073_709_551_615n;

export function comparePackageVersions(left: string, right: string): number {
  const leftVersion = parsePackageVersion(left);
  const rightVersion = parsePackageVersion(right);
  if (leftVersion && rightVersion) {
    const length = Math.max(leftVersion.numericCore.length, rightVersion.numericCore.length);
    for (let index = 0; index < length; index += 1) {
      const leftSegment = leftVersion.numericCore[index] ?? 0n;
      const rightSegment = rightVersion.numericCore[index] ?? 0n;
      if (leftSegment !== rightSegment) {
        return leftSegment < rightSegment ? -1 : 1;
      }
    }
    return comparePrerelease(leftVersion.prerelease, rightVersion.prerelease);
  }
  return compareText(left.toLowerCase(), right.toLowerCase());
}

type ParsedPackageVersion = {
  numericCore: bigint[];
  prerelease: string[] | null;
  canonical: string;
};

/** Parses a V1 package identity and returns its canonical spelling. */
export function canonicalizePackageVersion(value: string): string | null {
  return parsePackageVersion(value)?.canonical ?? null;
}

function parsePackageVersion(value: string): ParsedPackageVersion | null {
  const trimmed = value.trim();
  const match = /^(\d+(?:\.\d+){0,3})(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(trimmed);
  if (!match) {
    return null;
  }
  try {
    const captures: readonly (string | undefined)[] = match;
    const numericCore = captures[1];
    const prerelease = captures[2];
    if (numericCore === undefined) {
      return null;
    }
    const numericSegments = numericCore.split('.').map(BigInt);
    if (numericSegments.some((segment) => segment > MAX_U64)) {
      return null;
    }
    while (numericSegments.length < 3) {
      numericSegments.push(0n);
    }
    if (numericSegments.length === 4 && numericSegments[3] === 0n) {
      numericSegments.pop();
    }
    const normalizedPrerelease = prerelease?.toLowerCase().split('.') ?? null;
    if (
      normalizedPrerelease?.some(
        (identifier) =>
          /^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith('0'),
      )
    ) {
      return null;
    }
    const numericText = numericSegments.map(String).join('.');
    return {
      numericCore: numericSegments,
      prerelease: normalizedPrerelease,
      canonical:
        normalizedPrerelease === null
          ? numericText
          : `${numericText}-${normalizedPrerelease.join('.')}`,
    };
  } catch {
    return null;
  }
}

function comparePrerelease(left: string[] | null, right: string[] | null): number {
  if (left === null || right === null) {
    if (left === right) {
      return 0;
    }
    return left === null ? 1 : -1;
  }
  const sharedLength = Math.min(left.length, right.length);
  for (let index = 0; index < sharedLength; index += 1) {
    const ordering = comparePrereleaseIdentifier(left[index], right[index]);
    if (ordering !== 0) {
      return ordering;
    }
  }
  return left.length === right.length ? 0 : left.length < right.length ? -1 : 1;
}

function comparePrereleaseIdentifier(left: string, right: string): number {
  const leftNumeric = /^\d+$/.test(left);
  const rightNumeric = /^\d+$/.test(right);
  if (leftNumeric && rightNumeric) {
    const leftNumber = BigInt(left);
    const rightNumber = BigInt(right);
    return leftNumber === rightNumber ? 0 : leftNumber < rightNumber ? -1 : 1;
  }
  if (leftNumeric !== rightNumeric) {
    return leftNumeric ? -1 : 1;
  }
  return compareText(left, right);
}

function compareText(left: string, right: string): number {
  return left === right ? 0 : left < right ? -1 : 1;
}
