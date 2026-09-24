import type { NvapiProfileStatus } from '@features/nvapi-settings';
import { mockState, requireGameDetails } from '../desktop-state';
import { createInstallPathKey, resolveMock } from '../desktop-utils';
import { mockResolveGameExecutable } from './executables';

export function mockGetNvapiProfileStatus(gameId: string): Promise<NvapiProfileStatus> {
  return resolveMock(async () => {
    requireGameDetails(gameId);
    const selected = await mockResolveGameExecutable(gameId);
    const owned = mockState.nvapiProfileByGameId.get(gameId);
    if (!selected) {
      return {
        selectedExecutable: null,
        bindingPath: owned?.bindingPath ?? null,
        profileName: owned?.profileName ?? null,
        state: 'noExecutable',
        isPredefined: null,
        ownedByThisGame: owned !== undefined,
        canCreate: false,
        canDelete: false,
        pendingOperation: null,
        pendingOperationGameId: null,
        detail: null,
      };
    }
    const matchesOwned =
      owned !== undefined &&
      createInstallPathKey(owned.bindingPath) === createInstallPathKey(selected.absolute_path);
    return {
      selectedExecutable: selected.absolute_path,
      bindingPath: owned?.bindingPath ?? null,
      profileName: owned?.profileName ?? null,
      state: owned ? (matchesOwned ? 'owned' : 'conflict') : 'missing',
      isPredefined: owned ? false : null,
      ownedByThisGame: owned !== undefined,
      canCreate: !owned,
      canDelete: matchesOwned,
      pendingOperation: null,
      pendingOperationGameId: null,
      detail: null,
    };
  });
}

export function mockCreateNvapiProfile(gameId: string): Promise<void> {
  return resolveMock(async () => {
    const selected = await mockResolveGameExecutable(gameId);
    if (!selected || mockState.nvapiProfileByGameId.has(gameId)) {
      throw new Error('Mock profile cannot be created for the current executable.');
    }
    mockState.nvapiProfileByGameId.set(gameId, {
      profileName: `RenderPilot - ${requireGameDetails(gameId).game.identity.title}`,
      bindingPath: selected.absolute_path,
    });
  });
}

export function mockDeleteNvapiProfile(gameId: string): Promise<void> {
  return resolveMock(() => {
    requireGameDetails(gameId);
    if (!mockState.nvapiProfileByGameId.delete(gameId)) {
      throw new Error('Mock game has no RenderPilot profile to delete.');
    }
  });
}

export function mockMoveNvapiProfile(
  gameId: string,
  absolutePath: string,
  selectAutomatically: boolean,
): Promise<void> {
  return resolveMock(() => {
    requireGameDetails(gameId);
    const owned = mockState.nvapiProfileByGameId.get(gameId);
    if (!owned) {
      throw new Error('Mock game has no RenderPilot profile to move.');
    }
    owned.bindingPath = absolutePath;
    if (selectAutomatically) {
      mockState.executableOverrideByGameId.delete(gameId);
    } else {
      mockState.executableOverrideByGameId.set(gameId, absolutePath);
    }
  });
}
