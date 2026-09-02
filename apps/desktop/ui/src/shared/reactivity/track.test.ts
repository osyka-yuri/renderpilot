import { describe, expect, it } from 'vitest';
import { track } from './track';

describe('track', () => {
  it('accepts any number of dependency arguments and returns void', () => {
    track();
    track(1, 'two', { three: true }, null, undefined);
    expect(track).toBeTypeOf('function');
  });
});
