import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import {
  latestDownloadProgress,
  clearDownloadProgress,
  sumDownloadFractions,
  DOWNLOAD_PROGRESS_EVENT,
} from './download-progress.svelte';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

type ProgressPayload = { id: string; downloaded: number; total: number };
type ProgressHandler = (event: { payload: ProgressPayload }) => void;

let handler: ProgressHandler | undefined;

// The store registers its Tauri listener once per module lifetime, so the
// handler must be captured by an implementation that is installed before the
// first store call and never reset between tests — each test stays runnable
// in isolation (`it.only`).
(listen as Mock).mockImplementation((_event: string, h: ProgressHandler) => {
  handler = h;
  // eslint-disable-next-line @typescript-eslint/no-empty-function
  return Promise.resolve(() => {});
});

function emitProgress(payload: ProgressPayload): void {
  if (!handler) throw new Error('download-progress listener is not registered');
  handler({ payload });
}

describe('download-progress.svelte', () => {
  beforeEach(() => {
    clearDownloadProgress(['a', 'b', 'c']);
  });

  it('selects the freshest record among requested ids', () => {
    // Initializing the listener
    expect(latestDownloadProgress(['a'])).toBeNull();

    expect(listen).toHaveBeenCalledWith(DOWNLOAD_PROGRESS_EVENT, expect.any(Function));

    emitProgress({ id: 'a', downloaded: 10, total: 100 });
    emitProgress({ id: 'b', downloaded: 50, total: 100 });
    emitProgress({ id: 'a', downloaded: 20, total: 100 });

    // 'a' was updated last
    const bestForBoth = latestDownloadProgress(['a', 'b']);
    expect(bestForBoth).toEqual(expect.objectContaining({ id: 'a', downloaded: 20 }));

    // Requesting just 'b'
    const bestForB = latestDownloadProgress(['b']);
    expect(bestForB).toEqual(expect.objectContaining({ id: 'b', downloaded: 50 }));
  });

  it('returns null for unrelated ids', () => {
    latestDownloadProgress(['a']); // init listener
    emitProgress({ id: 'a', downloaded: 10, total: 100 });

    expect(latestDownloadProgress(['b'])).toBeNull();
  });

  it('clears specific progress ids', () => {
    latestDownloadProgress(['a']); // init listener
    emitProgress({ id: 'a', downloaded: 10, total: 100 });
    emitProgress({ id: 'b', downloaded: 20, total: 100 });

    clearDownloadProgress(['a']);

    expect(latestDownloadProgress(['a'])).toBeNull();
    expect(latestDownloadProgress(['b'])).toEqual(expect.objectContaining({ id: 'b' }));
  });

  describe('sumDownloadFractions', () => {
    it('sums per-id completion fractions for the requested ids', () => {
      sumDownloadFractions(['a']); // init listener
      emitProgress({ id: 'a', downloaded: 25, total: 100 });
      emitProgress({ id: 'b', downloaded: 50, total: 100 });
      emitProgress({ id: 'c', downloaded: 10, total: 100 });

      // Only 'a' and 'b' are requested: 0.25 + 0.5.
      expect(sumDownloadFractions(['a', 'b'])).toBeCloseTo(0.75);
    });

    it('ignores unknown ids and clamps each fraction to 1', () => {
      sumDownloadFractions(['a']); // init listener
      // Over-reported bytes must not push a single entry past 1.
      emitProgress({ id: 'a', downloaded: 150, total: 100 });

      expect(sumDownloadFractions(['a', 'missing'])).toBe(1);
    });

    it('treats a non-positive total as zero contribution', () => {
      sumDownloadFractions(['a']); // init listener
      emitProgress({ id: 'a', downloaded: 0, total: 0 });

      expect(sumDownloadFractions(['a'])).toBe(0);
    });
  });
});
