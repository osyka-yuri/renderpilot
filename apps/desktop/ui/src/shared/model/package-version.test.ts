import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { canonicalizePackageVersion, comparePackageVersions } from './package-version';

type PackageVersionCases = {
  parse: { input: string; canonical: string }[];
  rejected: { input: string }[];
  order: { lower: string; higher: string }[];
};

const cases = JSON.parse(
  readFileSync(
    new URL('../../../../../../testdata/package-version-cases.json', import.meta.url),
    'utf8',
  ),
) as PackageVersionCases;

describe('comparePackageVersions', () => {
  it('matches the shared canonicalization and rejection corpus', () => {
    for (const testCase of cases.parse) {
      expect(canonicalizePackageVersion(testCase.input)).toBe(testCase.canonical);
    }
    for (const testCase of cases.rejected) {
      expect(canonicalizePackageVersion(testCase.input)).toBeNull();
    }
  });

  it('matches the shared precedence corpus', () => {
    for (const testCase of cases.order) {
      expect(comparePackageVersions(testCase.lower, testCase.higher)).toBeLessThan(0);
    }
  });

  it('keeps arbitrary-precision numeric identifiers exact', () => {
    expect(
      comparePackageVersions(
        '1.0.0-preview.18446744073709551615',
        '1.0.0-preview.9999999999999999999',
      ),
    ).toBeGreaterThan(0);
    expect(
      comparePackageVersions('1.18446744073709551615', '1.18446744073709551614'),
    ).toBeGreaterThan(0);
  });

  it('uses deterministic text ordering for invalid wire values', () => {
    expect(comparePackageVersions('unknown-b', 'unknown-a')).toBeGreaterThan(0);
  });
});
