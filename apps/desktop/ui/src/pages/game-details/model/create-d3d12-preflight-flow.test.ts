import { describe, expect, it, vi } from 'vitest';

import { createD3d12PreflightFlow } from './create-d3d12-preflight-flow.svelte';

describe('createD3d12PreflightFlow', () => {
  it('keeps the captured pending value and closes recovery after a successful retry', async () => {
    let resolveRetry!: (value: { kind: 'ready'; value: string }) => void;
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockImplementationOnce(
        () =>
          new Promise<{ kind: 'ready'; value: string }>((resolve) => {
            resolveRetry = resolve;
          }),
      );
    const onReady = vi.fn();
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => true,
      onReady,
      onError: vi.fn(),
    });

    await flow.start({ gameId: 'game' });
    expect(flow.developerModeOpen).toBe(true);
    expect(flow.pendingRecovery).toEqual({ gameId: 'game' });

    const retry = flow.retry();
    expect(flow.developerModeRetrying).toBe(true);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(false);

    resolveRetry({ kind: 'ready', value: 'fresh' });
    await retry;

    expect(prepare).toHaveBeenLastCalledWith({ gameId: 'game' });
    expect(onReady).toHaveBeenCalledWith({ gameId: 'game' }, 'fresh');
    expect(flow.developerModeOpen).toBe(false);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(false);
    expect(flow.planning).toBe(false);
  });

  it('does not finish start before an asynchronous ready handler completes', async () => {
    let resolveReady!: () => void;
    const onReady = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveReady = resolve;
        }),
    );
    const flow = createD3d12PreflightFlow({
      prepare: vi.fn().mockResolvedValue({ kind: 'ready' as const, value: 'fresh' }),
      isCurrent: () => true,
      onReady,
      onError: vi.fn(),
    });
    let settled = false;

    const start = flow.start('pending').then(() => {
      settled = true;
    });
    await vi.waitFor(() => {
      expect(onReady).toHaveBeenCalledWith('pending', 'fresh');
    });

    expect(settled).toBe(false);
    expect(flow.planning).toBe(true);

    resolveReady();
    await start;

    expect(settled).toBe(true);
    expect(flow.planning).toBe(false);
  });

  it('keeps recovery open and unlocks retry after a transient exception', async () => {
    const failure = new Error('transport unavailable');
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockRejectedValueOnce(failure);
    const onError = vi.fn();
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => true,
      onReady: vi.fn(),
      onError,
    });

    await flow.start('pending');
    await flow.retry();

    expect(onError).toHaveBeenCalledWith(failure, 'pending');
    expect(flow.developerModeOpen).toBe(true);
    expect(flow.developerModeRetrying).toBe(false);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(false);
  });

  it('reports a disabled retry only after the retry result arrives', async () => {
    let resolveRetry!: (value: {
      kind: 'blocked';
      blockers: ['developer_mode_required'];
      recovery: 'developer_mode_required';
    }) => void;
    let resolveNextRetry!: (value: { kind: 'ready'; value: string }) => void;
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockImplementationOnce(
        () =>
          new Promise<{
            kind: 'blocked';
            blockers: ['developer_mode_required'];
            recovery: 'developer_mode_required';
          }>((resolve) => {
            resolveRetry = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<{ kind: 'ready'; value: string }>((resolve) => {
            resolveNextRetry = resolve;
          }),
      );
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => true,
      onReady: vi.fn(),
      onError: vi.fn(),
    });

    await flow.start('pending');
    const retry = flow.retry();

    expect(flow.developerModeRetrying).toBe(true);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(false);

    resolveRetry({
      kind: 'blocked',
      blockers: ['developer_mode_required'],
      recovery: 'developer_mode_required',
    });
    await retry;

    expect(flow.developerModeRetrying).toBe(false);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(true);

    const nextRetry = flow.retry();
    expect(flow.developerModeRetrying).toBe(true);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(true);

    resolveNextRetry({ kind: 'ready', value: 'fresh' });
    await nextRetry;

    expect(flow.developerModeOpen).toBe(false);
    expect(flow.developerModeStillDisabledAfterRetry).toBe(false);
  });

  it('allows cancellation during retry and ignores its late result', async () => {
    let resolveRetry!: (value: { kind: 'ready'; value: string }) => void;
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockImplementationOnce(
        () =>
          new Promise<{ kind: 'ready'; value: string }>((resolve) => {
            resolveRetry = resolve;
          }),
      );
    const onReady = vi.fn();
    const onCancel = vi.fn();
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => true,
      onReady,
      onError: vi.fn(),
      onCancel,
    });

    await flow.start('pending');
    const retry = flow.retry();
    flow.cancel();
    resolveRetry({ kind: 'ready', value: 'late' });
    await retry;

    expect(onCancel).toHaveBeenCalledWith('pending');
    expect(onReady).not.toHaveBeenCalled();
    expect(flow.developerModeOpen).toBe(false);
    expect(flow.planning).toBe(false);
  });

  it('does not let an older generation clear newer recovery state', async () => {
    let resolveFirst!: (value: { kind: 'ready'; value: string }) => void;
    const prepare = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<{ kind: 'ready'; value: string }>((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      });
    const onReady = vi.fn();
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => true,
      onReady,
      onError: vi.fn(),
    });

    const first = flow.start('first');
    flow.cancel();
    await flow.start('second');
    expect(flow.pendingRecovery).toBe('second');

    resolveFirst({ kind: 'ready', value: 'late' });
    await first;

    expect(onReady).not.toHaveBeenCalled();
    expect(flow.developerModeOpen).toBe(true);
    expect(flow.pendingRecovery).toBe('second');
  });

  it('closes recovery and unlocks retry when its owner becomes stale', async () => {
    let current = true;
    let resolveRetry!: (value: {
      kind: 'blocked';
      blockers: ['developer_mode_required'];
      recovery: 'developer_mode_required';
    }) => void;
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockImplementationOnce(
        () =>
          new Promise<{
            kind: 'blocked';
            blockers: ['developer_mode_required'];
            recovery: 'developer_mode_required';
          }>((resolve) => {
            resolveRetry = resolve;
          }),
      );
    const flow = createD3d12PreflightFlow({
      prepare,
      isCurrent: () => current,
      onReady: vi.fn(),
      onError: vi.fn(),
    });

    await flow.start('pending');
    const retry = flow.retry();
    current = false;
    resolveRetry({
      kind: 'blocked',
      blockers: ['developer_mode_required'],
      recovery: 'developer_mode_required',
    });
    await retry;

    expect(flow.developerModeOpen).toBe(false);
    expect(flow.developerModeRetrying).toBe(false);
    expect(flow.planning).toBe(false);
  });
});
