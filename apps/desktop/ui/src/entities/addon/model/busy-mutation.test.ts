import { describe, expect, it } from 'vitest';

import {
  isMutationFailure,
  isMutationSuccess,
  resolveShouldProbe,
  shouldSkipBusyMutation,
} from './busy-mutation';

describe('shouldSkipBusyMutation', () => {
  it('skips when already busy', () => {
    expect(shouldSkipBusyMutation(true, undefined, false)).toBe(true);
    expect(shouldSkipBusyMutation(true, true, true)).toBe(true);
  });

  it('skips update-gated calls when no update is available', () => {
    expect(shouldSkipBusyMutation(false, true, false)).toBe(true);
  });

  it('allows update-gated calls when an update is available', () => {
    expect(shouldSkipBusyMutation(false, true, true)).toBe(false);
  });

  it('allows non-gated calls regardless of update availability', () => {
    expect(shouldSkipBusyMutation(false, undefined, false)).toBe(false);
    expect(shouldSkipBusyMutation(false, false, false)).toBe(false);
  });
});

describe('resolveShouldProbe', () => {
  it('defaults from store postMutationProbe when call omits probeUpdates', () => {
    expect(resolveShouldProbe(undefined, 'passive')).toBe(true);
    expect(resolveShouldProbe(undefined, 'never')).toBe(false);
  });

  it('lets the call override the store default', () => {
    expect(resolveShouldProbe(true, 'never')).toBe(true);
    expect(resolveShouldProbe(false, 'passive')).toBe(false);
  });
});

describe('mutation result helpers', () => {
  it('classifies ok / skipped / failed', () => {
    expect(isMutationSuccess('ok')).toBe(true);
    expect(isMutationSuccess('skipped')).toBe(false);
    expect(isMutationSuccess('failed')).toBe(false);
    expect(isMutationFailure('failed')).toBe(true);
    expect(isMutationFailure('ok')).toBe(false);
    expect(isMutationFailure('skipped')).toBe(false);
  });
});
