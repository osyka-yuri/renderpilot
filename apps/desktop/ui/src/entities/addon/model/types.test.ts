import { describe, expect, it } from 'vitest';

import { isReshadeChannel } from './types';

describe('isReshadeChannel', () => {
  it('accepts public channels and rejects unknown persisted values', () => {
    expect(isReshadeChannel('stable')).toBe(true);
    expect(isReshadeChannel('nightly')).toBe(true);
    expect(isReshadeChannel('legacy-preview')).toBe(false);
    expect(isReshadeChannel(null)).toBe(false);
  });
});
