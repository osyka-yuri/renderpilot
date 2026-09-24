import { describe, expect, it } from 'vitest';

import { canRestoreOriginal } from './original-state';

const current = { wire: 'default', label: 'Default', dword: 0 };

describe('canRestoreOriginal', () => {
  it('enables restore when explicit presence differs even if DWORDs match', () => {
    expect(canRestoreOriginal({ present: false, value: null }, current, true)).toBe(true);
  });

  it('does not enable restore when presence and explicit value already match', () => {
    expect(
      canRestoreOriginal(
        { present: true, value: { wire: 'default', label: 'Default', dword: 0 } },
        current,
        true,
      ),
    ).toBe(false);
  });

  it('does not offer restore when the live presence could not be confirmed', () => {
    expect(canRestoreOriginal({ present: false, value: null }, current, null)).toBe(false);
  });
});
