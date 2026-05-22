import { describe, expect, it, vi } from 'vitest';

import { createExclusiveTaskRunner } from './exclusive-task-runner.svelte';

describe('createExclusiveTaskRunner', () => {
  it('returns task result when not busy', async () => {
    const runner = createExclusiveTaskRunner();
    const result = await runner.run(() => Promise.resolve(42));

    expect(result).toBe(42);
  });

  it('returns null when already busy', async () => {
    const runner = createExclusiveTaskRunner();

    let resolveTask = (_value: number): void => {
      throw new Error('resolveTask not set');
    };
    const task = () =>
      new Promise<number>((resolve) => {
        resolveTask = resolve;
      });

    const first = runner.run(task);
    const second = runner.run(() => Promise.resolve(99));

    expect(runner.busy).toBe(true);
    expect(await second).toBeNull();

    resolveTask(1);
    expect(await first).toBe(1);
    expect(runner.busy).toBe(false);
  });

  it('calls onError and returns null when task throws', async () => {
    const onError = vi.fn();
    const runner = createExclusiveTaskRunner({ onError });

    const error = new Error('task failed');
    const result = await runner.run(() => Promise.reject(error));

    expect(result).toBeNull();
    expect(onError).toHaveBeenCalledWith(error);
    expect(runner.busy).toBe(false);
  });

  it('resets busy in finally even when task throws', async () => {
    const runner = createExclusiveTaskRunner();

    await runner.run(() => Promise.reject(new Error('boom')));

    expect(runner.busy).toBe(false);
  });

  it('allows per-run onBeforeRun override', async () => {
    const defaultOnBeforeRun = vi.fn();
    const overrideOnBeforeRun = vi.fn();
    const runner = createExclusiveTaskRunner({ onBeforeRun: defaultOnBeforeRun });

    await runner.run(() => Promise.resolve(1), { onBeforeRun: overrideOnBeforeRun });

    expect(defaultOnBeforeRun).not.toHaveBeenCalled();
    expect(overrideOnBeforeRun).toHaveBeenCalled();
  });

  it('allows per-run onError override', async () => {
    const defaultOnError = vi.fn();
    const overrideOnError = vi.fn();
    const runner = createExclusiveTaskRunner({ onError: defaultOnError });

    await runner.run(() => Promise.reject(new Error('boom')), { onError: overrideOnError });

    expect(defaultOnError).not.toHaveBeenCalled();
    expect(overrideOnError).toHaveBeenCalled();
  });
});
