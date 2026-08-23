import { describe, expect, it } from 'vitest';

import { formatReleaseVersionLabel } from './release-version-label';

describe('formatReleaseVersionLabel', () => {
  it('renders canonical release labels supplied by the backend', () => {
    expect(
      formatReleaseVersionLabel({
        version: '1.1.3',
        releaseLabel: 'revision b',
        unknownLabel: 'Unknown',
      }),
    ).toBe('v1.1.3 (revision b)');
  });

  it('renders debug release labels directly from backend identity', () => {
    expect(
      formatReleaseVersionLabel({
        version: '3.7.10',
        releaseLabel: 'Debug',
        unknownLabel: 'Unknown',
      }),
    ).toBe('v3.7.10 (Debug)');
  });

  it('renders plain version without parentheses when no label is present', () => {
    expect(
      formatReleaseVersionLabel({
        version: '3.8.0',
        releaseLabel: null,
        unknownLabel: 'Unknown',
      }),
    ).toBe('v3.8.0');
  });

  it('does not infer a version or label when the backend reports unknown', () => {
    expect(
      formatReleaseVersionLabel({
        version: null,
        releaseLabel: 'revision b',
        unknownLabel: 'Unknown',
      }),
    ).toBe('Unknown');
  });
});
