import type { AppUpdateDownloadEvent } from '../api/app-updater-gateway';
import type { DownloadProgressView } from './types';

export type DownloadProgressState = {
  totalBytes: number | null;
  receivedBytes: number;
  networkFinished: boolean;
};

export const EMPTY_PROGRESS: DownloadProgressState = {
  totalBytes: null,
  receivedBytes: 0,
  networkFinished: false,
};

function positiveFinite(value: number): boolean {
  return Number.isFinite(value) && value > 0;
}

export function applyDownloadEvent(
  state: DownloadProgressState,
  event: AppUpdateDownloadEvent,
): DownloadProgressState {
  switch (event.type) {
    case 'started': {
      const contentLength = event.contentLength;
      return {
        totalBytes: contentLength !== null && positiveFinite(contentLength) ? contentLength : null,
        receivedBytes: 0,
        networkFinished: false,
      };
    }
    case 'progress': {
      const chunk = positiveFinite(event.chunkLength) ? event.chunkLength : 0;
      return {
        ...state,
        receivedBytes: Math.max(0, state.receivedBytes + chunk),
      };
    }
    case 'finished':
      return {
        ...state,
        networkFinished: true,
      };
  }
}

/** In-flight progress for the dialog — never invents 100% without evidence. */
export function toProgressView(state: DownloadProgressState): DownloadProgressView {
  const { totalBytes, receivedBytes, networkFinished } = state;

  let ratio: number | null = null;
  if (totalBytes !== null && totalBytes > 0) {
    const raw = networkFinished ? 1 : receivedBytes / totalBytes;
    ratio = clamp(raw, 0, 1);
  }

  return {
    ratio,
    receivedBytes,
    totalBytes,
    networkFinished,
  };
}

/**
 * Display-only 100% frame for the last busy paint before install exit.
 * Does not mutate download state; only snaps received bytes up to total when known.
 */
export function toCompletedProgressView(state: DownloadProgressState): DownloadProgressView {
  const totalBytes = state.totalBytes;
  const receivedBytes =
    totalBytes !== null ? Math.max(state.receivedBytes, totalBytes) : state.receivedBytes;

  return {
    ratio: 1,
    receivedBytes,
    totalBytes,
    networkFinished: true,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
