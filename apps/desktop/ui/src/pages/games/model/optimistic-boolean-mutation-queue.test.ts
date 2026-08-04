import { describe, expect, it, vi } from 'vitest';

import { createGameSummary } from '@entities/game';

import { createOptimisticBooleanMutationQueue } from './optimistic-boolean-mutation-queue';

describe('createOptimisticBooleanMutationQueue', () => {
  it('serializes writes per game and preserves the first confirmed snapshot', async () => {
    const queue = createOptimisticBooleanMutationQueue();
    const card = createGameSummary({ game_id: 'game', is_favorite: false });
    const calls: string[] = [];
    const firstWrite = Promise.withResolvers<undefined>();

    const first = queue.begin('game', card, 0, 'request', false, true);
    const firstOperation = queue.enqueue('game', async () => {
      calls.push('first');
      await firstWrite.promise;
    });
    const second = queue.begin('game', card, 0, 'request', true, false);
    const secondPersist = vi.fn(() => {
      calls.push('second');
      return Promise.resolve();
    });
    const secondOperation = queue.enqueue('game', secondPersist);

    expect(first.mutation).toBe(second.mutation);
    expect(second.mutation?.confirmedValue).toBe(false);
    expect(queue.isLatest('game', first.token)).toBe(false);
    expect(queue.isLatest('game', second.token)).toBe(true);
    expect(secondPersist).not.toHaveBeenCalled();

    firstWrite.resolve(undefined);
    await Promise.all([firstOperation, secondOperation]);
    expect(calls).toEqual(['first', 'second']);
  });

  it('continues the queue after a rejected write', async () => {
    const queue = createOptimisticBooleanMutationQueue();
    const rejected = queue.enqueue('game', () => Promise.reject(new Error('failed')));
    const next = vi.fn(() => Promise.resolve('saved'));
    const recovered = queue.enqueue('game', next);

    await expect(rejected).rejects.toThrow('failed');
    await expect(recovered).resolves.toBe('saved');
    expect(next).toHaveBeenCalledOnce();
  });
});
