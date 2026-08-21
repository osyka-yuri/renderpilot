import { describe, expect, it, vi } from 'vitest';

import type { LibraryPackageState } from '@entities/library';
import { ClientError } from '@shared/errors';

import { executeGraphicsSwap } from './execute-graphics-swap';

describe('executeGraphicsSwap', () => {
  it('downloads the artifact before applying when it is not yet downloaded', async () => {
    const downloadArtifact = vi.fn(() =>
      Promise.resolve({
        package_id: 'package:original',
        version: '3.7.0',
        is_downloaded: true,
        artifact_id: 'artifact:downloaded',
      } satisfies LibraryPackageState),
    );
    const applySwap = vi.fn(() =>
      Promise.resolve({
        game_id: 'game-1',
        component_id: 'component-1',
        applied_path: 'C:/game/file.dll',
        replacement_path: 'C:/repo/file.dll',
        updated_file_count: 1,
        d3d12_executable_action: null,
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
        updated_file_count: 1,
        d3d12_executable_action: null,
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

  it('turns a malformed download response into a stable client contract error', async () => {
    const response = {
      package_id: 'package:original',
      version: '3.7.0',
      is_downloaded: true,
      artifact_id: null,
    } satisfies LibraryPackageState;

    const error = await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'component-1',
        artifactId: 'artifact:original',
        isDownloaded: false,
      },
      {
        downloadArtifact: () => Promise.resolve(response),
        applySwap: vi.fn(),
      },
    ).catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(ClientError);
    expect(error).toMatchObject({
      code: 'graphics_swap_response_invalid',
      cause: response,
    });
  });

  it('passes the preflight fingerprint when the executable will change', async () => {
    const applySwap = vi.fn(() =>
      Promise.resolve({
        game_id: 'game-1',
        component_id: 'd3d12',
        applied_path: 'C:/game/D3D12Core.dll',
        replacement_path: 'C:/repo/D3D12Core.dll',
        updated_file_count: 1,
        d3d12_executable_action: null,
      }),
    );

    await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'd3d12',
        artifactId: 'artifact:619',
        isDownloaded: true,
        confirmationToken: 'fresh-fingerprint',
      },
      {
        applySwap,
        downloadArtifact: vi.fn(),
      },
    );

    expect(applySwap).toHaveBeenCalledWith('game-1', 'd3d12', 'artifact:619', 'fresh-fingerprint');
  });

  it('forwards the fresh game safety context to a file mutation', async () => {
    const applySwap = vi.fn(() =>
      Promise.resolve({
        game_id: 'game-1',
        component_id: 'component-1',
        applied_path: 'C:/game/file.dll',
        replacement_path: 'C:/repo/file.dll',
        updated_file_count: 1,
        d3d12_executable_action: null,
      }),
    );

    await executeGraphicsSwap(
      {
        gameId: 'game-1',
        componentId: 'component-1',
        artifactId: 'artifact:ready',
        isDownloaded: true,
        gameContextToken: 'game-safety-token',
      },
      { applySwap, downloadArtifact: vi.fn() },
    );

    expect(applySwap).toHaveBeenCalledWith(
      'game-1',
      'component-1',
      'artifact:ready',
      undefined,
      'game-safety-token',
    );
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
