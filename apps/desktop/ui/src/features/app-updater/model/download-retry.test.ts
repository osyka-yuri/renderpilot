import { describe, expect, it, vi } from 'vitest';

import type { AppUpdateDownloadEvent } from '../api/app-updater-gateway';
import { downloadWithRetries, type DownloadRetryScheduled } from './download-retry';
import type { DownloadProgressState } from './progress';

const ACTIVE = () => true;

describe('downloadWithRetries', () => {
  it('resets progress and succeeds on the second attempt after the fixed delay', async () => {
    let attempts = 0;
    const delays: number[] = [];
    const progress: DownloadProgressState[] = [];

    const result = await downloadWithRetries({
      download: (onEvent) => {
        attempts += 1;
        onEvent({ type: 'started', contentLength: 10 });
        onEvent({ type: 'progress', chunkLength: attempts === 1 ? 4 : 10 });
        if (attempts === 1) {
          return Promise.reject(new Error('network'));
        }
        onEvent({ type: 'finished' });
        return Promise.resolve();
      },
      isActive: ACTIVE,
      waitBeforeRetry: (delayMs) => {
        delays.push(delayMs);
        return Promise.resolve();
      },
      onProgress: (next) => {
        progress.push(next);
      },
      onRetryScheduled: vi.fn(),
    });

    expect(result).toEqual({
      status: 'completed',
      attempts: 2,
      progress: { totalBytes: 10, receivedBytes: 10, networkFinished: true },
    });
    expect(delays).toEqual([1_000]);
    expect(
      progress.filter(
        (state) => state.totalBytes === null && state.receivedBytes === 0 && !state.networkFinished,
      ),
    ).toHaveLength(2);
  });

  it('ignores queued events from a failed attempt during backoff and the next attempt', async () => {
    const retryWait = Promise.withResolvers<undefined>();
    const secondDownload = Promise.withResolvers<undefined>();
    const progress = vi.fn();
    let attempts = 0;
    let firstAttemptEvent: ((event: AppUpdateDownloadEvent) => void) | undefined;
    let secondAttemptEvent: ((event: AppUpdateDownloadEvent) => void) | undefined;

    const download = downloadWithRetries({
      download: (onEvent) => {
        attempts += 1;
        if (attempts === 1) {
          firstAttemptEvent = onEvent;
          return Promise.reject(new Error('network'));
        }
        secondAttemptEvent = onEvent;
        return secondDownload.promise;
      },
      isActive: ACTIVE,
      waitBeforeRetry: () => retryWait.promise,
      onProgress: progress,
      onRetryScheduled: vi.fn(),
    });

    await vi.waitFor(() => {
      expect(attempts).toBe(1);
    });
    const callsDuringBackoff = progress.mock.calls.length;
    firstAttemptEvent?.({ type: 'progress', chunkLength: 5 });
    expect(progress).toHaveBeenCalledTimes(callsDuringBackoff);

    retryWait.resolve(undefined);
    await vi.waitFor(() => {
      expect(attempts).toBe(2);
    });
    const callsDuringSecondAttempt = progress.mock.calls.length;
    firstAttemptEvent?.({ type: 'finished' });
    expect(progress).toHaveBeenCalledTimes(callsDuringSecondAttempt);

    secondAttemptEvent?.({ type: 'started', contentLength: 10 });
    secondAttemptEvent?.({ type: 'progress', chunkLength: 10 });
    secondAttemptEvent?.({ type: 'finished' });
    secondDownload.resolve(undefined);

    await expect(download).resolves.toMatchObject({ status: 'completed', attempts: 2 });
  });

  it('returns the final download error after exhausting all attempts', async () => {
    const delays: number[] = [];
    const failures = [new Error('first'), new Error('second'), new Error('third')];
    const scheduledRetries: DownloadRetryScheduled[] = [];
    let attempts = 0;

    const result = await downloadWithRetries({
      download: () => Promise.reject(failures[attempts++]),
      isActive: ACTIVE,
      waitBeforeRetry: (delayMs) => {
        delays.push(delayMs);
        return Promise.resolve();
      },
      onProgress: vi.fn(),
      onRetryScheduled: (retry) => {
        scheduledRetries.push(retry);
      },
    });

    expect(result).toEqual({
      status: 'failed',
      attempts: 3,
      reason: 'download',
      error: failures[2],
    });
    expect(delays).toEqual([1_000, 2_000]);
    expect(scheduledRetries.map((retry) => retry.failedAttempt)).toEqual([1, 2]);
  });

  it('cancels without another attempt when the operation becomes inactive during backoff', async () => {
    const retryWait = Promise.withResolvers<undefined>();
    let active = true;
    const downloadAttempt = vi.fn(() => Promise.reject(new Error('network')));

    const download = downloadWithRetries({
      download: downloadAttempt,
      isActive: () => active,
      waitBeforeRetry: () => retryWait.promise,
      onProgress: vi.fn(),
      onRetryScheduled: vi.fn(),
    });

    await vi.waitFor(() => {
      expect(downloadAttempt).toHaveBeenCalledTimes(1);
    });
    active = false;
    retryWait.resolve(undefined);

    await expect(download).resolves.toEqual({ status: 'cancelled' });
    expect(downloadAttempt).toHaveBeenCalledTimes(1);
  });

  it('returns an explicit failure when waiting before a retry fails', async () => {
    const waitError = new Error('timer unavailable');
    const downloadAttempt = vi.fn(() => Promise.reject(new Error('network')));

    const result = await downloadWithRetries({
      download: downloadAttempt,
      isActive: ACTIVE,
      waitBeforeRetry: () => Promise.reject(waitError),
      onProgress: vi.fn(),
      onRetryScheduled: vi.fn(),
    });

    expect(result).toEqual({
      status: 'failed',
      attempts: 1,
      reason: 'retry-wait',
      error: waitError,
    });
    expect(downloadAttempt).toHaveBeenCalledTimes(1);
  });
});
