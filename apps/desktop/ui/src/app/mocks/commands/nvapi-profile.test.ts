import type { NvapiProfileStatus } from '@features/nvapi-settings';
import { beforeEach, describe, expect, it } from 'vitest';

import { mockInvoker, resetMockDesktopState } from '../desktop';
import { mockState } from '../desktop-state';

const GAME_ID = 'steam:1091500';
const UNKNOWN_GAME_ID = 'steam:missing';

describe('preview NVIDIA profile commands', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('rejects deletion without an owned profile', async () => {
    await expect(
      mockInvoker<undefined>('delete_nvapi_profile', { gameId: GAME_ID }),
    ).rejects.toThrow('no RenderPilot profile to delete');
  });

  it('rejects mutations for an unknown game even if a stale profile entry exists', async () => {
    mockState.nvapiProfileByGameId.set(UNKNOWN_GAME_ID, {
      profileName: 'Stale profile',
      bindingPath: 'C:/Games/Stale/Game.exe',
    });

    await expect(
      mockInvoker<undefined>('delete_nvapi_profile', { gameId: UNKNOWN_GAME_ID }),
    ).rejects.toThrow(`could not find game ${UNKNOWN_GAME_ID}`);
    await expect(
      mockInvoker<undefined>('move_nvapi_profile', {
        gameId: UNKNOWN_GAME_ID,
        absolutePath: 'C:/Games/Stale/New.exe',
        selectAutomatically: false,
      }),
    ).rejects.toThrow(`could not find game ${UNKNOWN_GAME_ID}`);
    expect(mockState.nvapiProfileByGameId.get(UNKNOWN_GAME_ID)?.bindingPath).toBe(
      'C:/Games/Stale/Game.exe',
    );
  });

  it('recognizes equivalent Windows path spellings for an owned profile', async () => {
    await mockInvoker<undefined>('create_nvapi_profile', { gameId: GAME_ID });
    const owned = mockState.nvapiProfileByGameId.get(GAME_ID);
    if (!owned) {
      throw new Error('Fixture prerequisite: expected an owned profile.');
    }
    owned.bindingPath = owned.bindingPath.replaceAll('/', '\\').toUpperCase();

    await expect(
      mockInvoker<NvapiProfileStatus>('get_nvapi_profile_status', { gameId: GAME_ID }),
    ).resolves.toMatchObject({ state: 'owned', canDelete: true });
  });
});
