/**
 * Runs `worker` over `items` through a bounded worker pool: at most `limit`
 * invocations are in flight at once. Workers pull from a shared cursor, so the
 * pool stays full until every item is processed.
 *
 * This helper does not catch — throwing is the worker's responsibility. Callers
 * that must not abort the batch on a single failure should handle errors inside
 * the worker (e.g. record them and resolve).
 */
export async function runWithConcurrency<T>(
  items: readonly T[],
  limit: number,
  worker: (item: T, index: number) => Promise<void>,
): Promise<void> {
  const workerCount = boundedWorkerCount(limit, items.length);
  let cursor = 0;

  // Reading and advancing the cursor is synchronous (no await between), so each
  // index is claimed by exactly one worker.
  const runner = async (): Promise<void> => {
    for (;;) {
      const index = cursor;
      cursor += 1;

      if (index >= items.length) {
        return;
      }

      await worker(items[index], index);
    }
  };

  await Promise.all(Array.from({ length: workerCount }, () => runner()));
}

/**
 * Normalizes a concurrency limit to the number of workers to spawn: a positive
 * finite integer, never more than there is work for. Throws on an invalid limit
 * so a misconfiguration surfaces immediately rather than silently running
 * serially.
 */
function boundedWorkerCount(limit: number, itemCount: number): number {
  const normalized = Math.floor(limit);

  if (!Number.isFinite(limit) || normalized < 1) {
    throw new RangeError('runWithConcurrency limit must be a positive finite number.');
  }

  return Math.min(normalized, itemCount);
}
