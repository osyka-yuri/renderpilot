import { afterEach, describe, expect, it, vi } from 'vitest';

import { createLazyPageResource } from './lazy-page-resource.svelte';

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
};

function createDeferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });

  return { promise, resolve, reject };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('createLazyPageResource', () => {
  it('deduplicates concurrent requests and caches the loaded component', async () => {
    const pending = createDeferred<string>();
    const loader = vi.fn(() => pending.promise);
    const page = createLazyPageResource({ id: 'settings', loader });

    const preload = page.preload();
    const activation = page.activate();
    await Promise.resolve();

    expect(loader).toHaveBeenCalledTimes(1);
    expect(page.state).toEqual({ status: 'loading' });

    pending.resolve('settings-component');
    await Promise.all([preload, activation]);

    expect(page.state).toEqual({
      status: 'ready',
      component: 'settings-component',
    });

    await page.preload();
    await page.activate();
    expect(loader).toHaveBeenCalledTimes(1);
  });

  it('promotes an in-flight preload to a foreground failure', async () => {
    const error = new Error('missing chunk');
    const pending = createDeferred<string>();
    const loader = vi.fn(() => pending.promise);
    const logError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const page = createLazyPageResource({ id: 'details', loader });

    const preload = page.preload();
    const activation = page.activate();
    pending.reject(error);
    await Promise.all([preload, activation]);

    expect(page.state).toEqual({ status: 'error', error });
    expect(logError).toHaveBeenCalledOnce();
  });

  it('keeps a speculative failure hidden and blocks repeated hover retries', async () => {
    const firstError = new Error('preload failed');
    const loader = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(firstError)
      .mockResolvedValueOnce('details-component');
    const logWarning = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const page = createLazyPageResource({ id: 'details', loader });

    await page.preload();

    expect(page.state).toEqual({ status: 'idle' });
    expect(logWarning).toHaveBeenCalledOnce();

    await page.preload();
    await page.preload();
    expect(loader).toHaveBeenCalledTimes(1);

    await page.activate();
    expect(loader).toHaveBeenCalledTimes(2);
    expect(page.state).toEqual({
      status: 'ready',
      component: 'details-component',
    });
  });

  it('surfaces a foreground failure and recovers through an explicit retry', async () => {
    const error = new Error('load failed');
    const loader = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce('libraries-component');
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const page = createLazyPageResource({ id: 'libraries', loader });

    await page.activate();
    expect(page.state).toEqual({ status: 'error', error });

    await page.retry();
    expect(loader).toHaveBeenCalledTimes(2);
    expect(page.state).toEqual({
      status: 'ready',
      component: 'libraries-component',
    });
  });

  it('captures synchronous loader failures without rejecting its public methods', async () => {
    const error = new Error('synchronous failure');
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const page = createLazyPageResource<string>({
      id: 'operations',
      loader: () => {
        throw error;
      },
    });

    await expect(page.activate()).resolves.toBeUndefined();
    expect(page.state).toEqual({ status: 'error', error });
  });
});
