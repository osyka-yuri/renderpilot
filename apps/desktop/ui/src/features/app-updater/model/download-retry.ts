import type { AppUpdateDownloadEvent } from '../api/app-updater-gateway';
import { applyDownloadEvent, EMPTY_PROGRESS, type DownloadProgressState } from './progress';

const DOWNLOAD_RETRY_DELAYS_MS = [1_000, 2_000] as const;
const TOTAL_DOWNLOAD_ATTEMPTS = DOWNLOAD_RETRY_DELAYS_MS.length + 1;

export type DownloadRetryScheduled = {
  failedAttempt: number;
  totalAttempts: number;
  delayMs: number;
  error: unknown;
};

export type DownloadRetryFailure = {
  status: 'failed';
  attempts: number;
  reason: 'download' | 'retry-wait';
  error: unknown;
};

export type DownloadRetryResult =
  | {
      status: 'completed';
      attempts: number;
      progress: DownloadProgressState;
    }
  | {
      status: 'cancelled';
    }
  | DownloadRetryFailure;

export type DownloadWithRetriesOptions = {
  download: (onEvent: (event: AppUpdateDownloadEvent) => void) => Promise<void>;
  isActive: () => boolean;
  waitBeforeRetry: (delayMs: number) => Promise<void>;
  onProgress: (progress: DownloadProgressState) => void;
  onRetryScheduled: (retry: DownloadRetryScheduled) => void;
};

export function waitForDownloadRetry(delayMs: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, delayMs);
  });
}

/**
 * Downloads an update with a bounded retry policy.
 *
 * Each attempt gets a unique token so queued events from an earlier failed
 * request cannot mutate the current attempt's progress.
 */
export async function downloadWithRetries(
  options: DownloadWithRetriesOptions,
): Promise<DownloadRetryResult> {
  const { download, isActive, onProgress, onRetryScheduled, waitBeforeRetry } = options;
  let activeAttemptToken: object | null = null;
  let attemptIndex = 0;

  for (;;) {
    if (!isActive()) {
      return { status: 'cancelled' };
    }

    const attempt = attemptIndex + 1;
    const attemptToken = {};
    let progress: DownloadProgressState = { ...EMPTY_PROGRESS };
    activeAttemptToken = attemptToken;
    onProgress(progress);

    let outcome: { status: 'completed' } | { status: 'failed'; error: unknown };
    try {
      await download((event) => {
        if (!isActive() || activeAttemptToken !== attemptToken) {
          return;
        }

        progress = applyDownloadEvent(progress, event);
        onProgress(progress);
      });
      outcome = { status: 'completed' };
    } catch (error) {
      outcome = { status: 'failed', error };
    } finally {
      if (activeAttemptToken === attemptToken) {
        activeAttemptToken = null;
      }
    }

    if (!isActive()) {
      return { status: 'cancelled' };
    }

    if (outcome.status === 'completed') {
      return { status: 'completed', attempts: attempt, progress };
    }

    if (attemptIndex >= DOWNLOAD_RETRY_DELAYS_MS.length) {
      return {
        status: 'failed',
        attempts: attempt,
        reason: 'download',
        error: outcome.error,
      };
    }

    const retryDelayMs = DOWNLOAD_RETRY_DELAYS_MS[attemptIndex];
    onRetryScheduled({
      failedAttempt: attempt,
      totalAttempts: TOTAL_DOWNLOAD_ATTEMPTS,
      delayMs: retryDelayMs,
      error: outcome.error,
    });

    try {
      await waitBeforeRetry(retryDelayMs);
    } catch (error) {
      return {
        status: 'failed',
        attempts: attempt,
        reason: 'retry-wait',
        error,
      };
    }

    attemptIndex += 1;
  }
}
