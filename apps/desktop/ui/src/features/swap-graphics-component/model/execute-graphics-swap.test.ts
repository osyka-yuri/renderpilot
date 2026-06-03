import { describe, expect, it, vi } from 'vitest';

import type { LibraryState } from '@entities/library';

import { executeGraphicsSwap } from './execute-graphics-swap';

describe('executeGraphicsSwap', () => {
  it('downloads the artifact before applying when it is not yet downloaded', async () => {
    const downloadArtifact = vi.fn(() =>
      Promise.resolve({
        id: 'artifact:original',
        version: '3.7.0',
        is_downloaded: true,
        local_path: '/local/path',
        artifact_id: 'artifact:downloaded',
      } satisfies LibraryState),
    );
    const applySwap = vi.fn(() =>
      Promise.resolve({
        game_id: 'game-1',
        component_id: 'component-1',
        applied_path: 'C:/game/file.dll',
        replacement_path: 'C:/repo/file.dll',
      }),
    );

    const result = await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'component-1',
        artifactId: 'artifact:original',
        isDownloaded: false,
      },
      {
        downloadArtifact,
        applySwap,
      },
    );

    expect(downloadArtifact).toHaveBeenCalledWith('artifact:original');
    expect(applySwap).toHaveBeenCalledWith('game-1', 'component-1', 'artifact:downloaded');
    expect(result?.game_id).toBe('game-1');
  });

  it('applies directly without downloading when already downloaded', async () => {
    const downloadArtifact = vi.fn();
    const applySwap = vi.fn(() =>
      Promise.resolve({
        game_id: 'game-1',
        component_id: 'component-1',
        applied_path: 'C:/game/file.dll',
        replacement_path: 'C:/repo/file.dll',
      }),
    );

    const result = await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'component-1',
        artifactId: 'artifact:ready',
        isDownloaded: true,
      },
      {
        downloadArtifact,
        applySwap,
      },
    );

    expect(downloadArtifact).not.toHaveBeenCalled();
    expect(applySwap).toHaveBeenCalledWith('game-1', 'component-1', 'artifact:ready');
    expect(result?.game_id).toBe('game-1');
  });

  it('stops before apply when signal is aborted', async () => {
    const applySwap = vi.fn();
    const controller = new AbortController();
    controller.abort();

    const result = await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'component-1',
        artifactId: 'artifact-1',
        isDownloaded: true,
        signal: controller.signal,
      },
      {
        applySwap,
        downloadArtifact: vi.fn(),
      },
    );

    expect(applySwap).not.toHaveBeenCalled();
    expect(result).toBeNull();
  });
});
