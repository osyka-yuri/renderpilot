import { SvelteMap } from 'svelte/reactivity';
import { listen } from '@tauri-apps/api/event';

export type DownloadProgress = {
  id: string;
  downloaded: number;
  total: number;
};

type ProgressEntry = DownloadProgress & { seq: number };

/** Must match `DOWNLOAD_PROGRESS_EVENT` in `src-tauri/src/commands/mod.rs`. */
export const DOWNLOAD_PROGRESS_EVENT = 'download-progress';

// ---------------------------------------------------------------------------
// Module-level reactive state
// ---------------------------------------------------------------------------

const progressMap = new SvelteMap<string, ProgressEntry>();
let listenerStarted = false;
let seq = 0;

function ensureListener(): void {
  if (listenerStarted) return;
  listenerStarted = true;

  listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) => {
    const { id, downloaded, total } = event.payload;
    progressMap.set(id, { id, downloaded, total, seq: ++seq });
  }).catch((err: unknown) => {
    console.error('[download-progress] Failed to start listener:', err);
    // Allow retrying on the next call to ensureListener (e.g. next component mount).
    listenerStarted = false;
  });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Returns the freshest `DownloadProgress` entry among the given ids, or `null`
 * if none are present. "Freshest" is determined by the monotonic `seq` counter —
 * whichever id received a progress event most recently wins.
 *
 * Intentionally not exported from the entity index: only `DownloadProgressBar`
 * should call this.
 */
export function latestDownloadProgress(ids: readonly string[]): DownloadProgress | null {
  ensureListener();

  let best: ProgressEntry | null = null;

  for (const id of ids) {
    const entry = progressMap.get(id);
    if (entry && (best === null || entry.seq > best.seq)) {
      best = entry;
    }
  }

  return best;
}

/**
 * Removes progress entries for the given ids without touching entries for other
 * concurrent downloads. Call before starting a new download so a stale 100% bar
 * from a previous run doesn't flash.
 */
export function clearDownloadProgress(ids: readonly string[]): void {
  for (const id of ids) {
    progressMap.delete(id);
  }
}

/**
 * Sums each id's completion fraction (downloaded/total, clamped to [0,1]) across
 * the given ids. Ids with no progress event yet (or a non-positive total)
 * contribute 0. Drives an aggregate progress bar across a batch of concurrent
 * downloads, where a known entry count is the denominator and each in-flight
 * download contributes its own byte progress.
 */
export function sumDownloadFractions(ids: readonly string[]): number {
  ensureListener();

  let sum = 0;

  for (const id of ids) {
    const entry = progressMap.get(id);
    if (entry && entry.total > 0) {
      sum += Math.min(1, entry.downloaded / entry.total);
    }
  }

  return sum;
}
