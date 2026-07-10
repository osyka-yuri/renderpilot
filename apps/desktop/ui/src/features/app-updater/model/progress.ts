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

export function toProgressView(state: DownloadProgressState): DownloadProgressView {
  const { totalBytes, receivedBytes, networkFinished } = state;

  let percent: number | null = null;
  if (totalBytes !== null && totalBytes > 0) {
    const raw = networkFinished ? 100 : (receivedBytes / totalBytes) * 100;
    percent = clamp(raw, 0, 100);
  }

  return {
    percent,
    receivedBytes,
    totalBytes,
    networkFinished,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
