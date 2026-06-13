import { describe, expect, it, vi } from 'vitest';

import { runWithConcurrency } from './run-with-concurrency';

describe('runWithConcurrency', () => {
  it('runs the worker once per item with its index', async () => {
    const seen: [item: number, index: number][] = [];

    await runWithConcurrency([10, 20, 30], 2, async (item, index) => {
      await Promise.resolve();
      seen.push([item, index]);
    });

    expect(seen.sort((a, b) => a[1] - b[1])).toEqual([
      [10, 0],
      [20, 1],
      [30, 2],
    ]);
  });

  it('never exceeds the concurrency limit', async () => {
    let active = 0;
    let maxActive = 0;
    const items = Array.from({ length: 12 }, (_, index) => index);

    await runWithConcurrency(items, 3, async () => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await Promise.resolve();
      await Promise.resolve();
      active -= 1;
    });

    expect(maxActive).toBe(3);
  });

  it('clamps the worker count to the item count', async () => {
    let active = 0;
    let maxActive = 0;

    await runWithConcurrency([1, 2], 8, async () => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await Promise.resolve();
      active -= 1;
    });

    expect(maxActive).toBeLessThanOrEqual(2);
  });

  it('is a no-op for an empty array', async () => {
    const worker = vi.fn();

    await runWithConcurrency([], 4, worker);

    expect(worker).not.toHaveBeenCalled();
  });

  it('rejects on an invalid limit', async () => {
    const worker = vi.fn();

    for (const limit of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      await expect(runWithConcurrency([1], limit, worker)).rejects.toBeInstanceOf(RangeError);
    }

    expect(worker).not.toHaveBeenCalled();
  });
});
