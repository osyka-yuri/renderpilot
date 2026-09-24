import { describe, expect, it } from 'vitest';

import { canResetToDriverDefault } from './setting-actions';

describe('canResetToDriverDefault', () => {
  it('allows deleting an explicit override even when its DWORD equals the default', () => {
    expect(canResetToDriverDefault(true)).toBe(true);
  });

  it('does not offer a no-op reset for an inherited nondefault value', () => {
    expect(canResetToDriverDefault(false)).toBe(false);
  });

  it('fails closed when explicit presence is unknown', () => {
    expect(canResetToDriverDefault(null)).toBe(false);
  });
});
