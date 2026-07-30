import { describe, expect, it } from 'vitest';

import { createHandle } from './app-updater-test-fixtures';
import { toOffer } from './offer';

describe('toOffer', () => {
  it('normalizes an RFC 3339 release timestamp at the gateway boundary', () => {
    expect(toOffer(createHandle({ date: '2026-07-11T12:00:00+02:00' })).releaseTimestamp).toBe(
      Date.UTC(2026, 6, 11, 10),
    );
  });

  it.each([null, '', '2026-02-30T00:00:00Z', '01/02/2026', 'not-a-date'])(
    'drops an absent, ambiguous or invalid release timestamp: %s',
    (date) => {
      expect(toOffer(createHandle({ date })).releaseTimestamp).toBeNull();
    },
  );
});
