import { describe, expect, it, vi } from 'vitest';

import { runUpdateAll, UpdateAllError } from './run-update-all';

const ITEM = {
  componentId: 'dlss',
  artifactId: 'artifact:new',
  isDownloaded: false,
};

describe('runUpdateAll', () => {
  it('supports an add-on-only batch and preserves store order', async () => {
    const events: string[] = [];
    const first = {
      update: vi.fn(() => {
        events.push('renodx');
        return Promise.resolve('ok' as const);
      }),
    };
    const second = {
      update: vi.fn(() => {
        events.push('luma');
        return Promise.resolve('ok' as const);
      }),
    };
    const onBulkSwap = vi.fn();

    await runUpdateAll({
      items: [],
      gameId: 'game:1',
      addonUpdates: [
        { step: 'renodx', store: first },
        { step: 'luma', store: second },
      ],
      onBulkSwap,
    });

    expect(events).toEqual(['renodx', 'luma']);
    expect(onBulkSwap).not.toHaveBeenCalled();
  });

  it('runs library updates before the captured add-on batch', async () => {
    const events: string[] = [];
    const store = {
      update: vi.fn(() => {
        events.push('addon');
        return Promise.resolve('ok' as const);
      }),
    };

    await runUpdateAll({
      items: [ITEM],
      gameId: 'game:1',
      addonUpdates: [{ step: 'renodx', store }],
      onBulkSwap: () => {
        events.push('libraries');
        return Promise.resolve();
      },
    });

    expect(events).toEqual(['libraries', 'addon']);
  });

  it('attempts later add-ons before reporting unexpected failures', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    const result = runUpdateAll({
      items: [ITEM],
      gameId: 'game:1',
      addonUpdates: [
        {
          step: 'renodx',
          store: { update: vi.fn(() => Promise.reject(new Error('addon failed'))) },
        },
        { step: 'luma', store: later },
      ],
      onBulkSwap: () => Promise.reject(new Error('libraries failed')),
    });

    await expect(result).rejects.toThrow('One or more update-all steps failed');
    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('treats store.update resolving failed as a failure and still runs later stores', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    const result = runUpdateAll({
      items: [],
      gameId: 'game:1',
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('failed' as const)) } },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    await expect(result).rejects.toThrow('One or more update-all steps failed');
    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('does not treat store.update skipped as a failure', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    await runUpdateAll({
      items: [],
      gameId: 'game:1',
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('skipped' as const)) } },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('attributes isolated failures to their workflow step', async () => {
    const result = runUpdateAll({
      items: [ITEM],
      gameId: 'game:1',
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('failed' as const)) } },
      ],
      onBulkSwap: () => Promise.reject(new Error('library failed')),
    });

    const error = await result.catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(UpdateAllError);
    expect((error as UpdateAllError).failures.map((failure) => failure.step)).toEqual([
      'libraries',
      'renodx',
    ]);
  });
});
