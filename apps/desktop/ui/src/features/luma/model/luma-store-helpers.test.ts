import { describe, expect, it } from 'vitest';

import { payloadRepairAction } from './luma-store-helpers';

describe('payloadRepairAction', () => {
  it('requires both torn state and a live installable profile', () => {
    expect(payloadRepairAction(false, true)).toBeUndefined();
    expect(payloadRepairAction(true, false)).toBeUndefined();
  });

  it('exposes an enabled repair only for a torn live profile', () => {
    expect(payloadRepairAction(true, true)).toMatchObject({ enabled: true });
  });
});
