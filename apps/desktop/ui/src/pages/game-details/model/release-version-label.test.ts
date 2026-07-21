import { describe, expect, it } from 'vitest';

import { formatReleaseVersionLabel } from './release-version-label';

describe('formatReleaseVersionLabel', () => {
  it('renders canonical release labels supplied by the backend', () => {
    expect(
      formatReleaseVersionLabel({
        version: '1.1.3',
        releaseLabel: 'revision b',
        isDebug: false,
        unknownLabel: 'Unknown',
      }),
    ).toBe('v1.1.3 (revision b)');
  });

  it('keeps debug presentation generic and ordered after the release label', () => {
    expect(
      formatReleaseVersionLabel({
        version: '1.1.3',
        releaseLabel: 'revision b',
        isDebug: true,
        unknownLabel: 'Unknown',
      }),
    ).toBe('v1.1.3 (revision b) (Debug)');
  });

  it('does not infer a version or label when the backend reports unknown', () => {
    expect(
      formatReleaseVersionLabel({
        version: null,
        releaseLabel: 'revision b',
        isDebug: false,
        unknownLabel: 'Unknown',
      }),
    ).toBe('Unknown');
  });
});
