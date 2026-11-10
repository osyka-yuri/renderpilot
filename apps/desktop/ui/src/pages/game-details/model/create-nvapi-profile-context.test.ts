import { beforeEach, describe, expect, it, vi } from 'vitest';

import { reportClientError } from '@shared/errors';
import {
  publishPresentedErrorNotification,
  publishWarningNotification,
} from '@shared/notifications';
import {
  createNvapiProfile,
  getNvapiProfileStatus,
  moveNvapiProfile,
  type NvapiProfileStatus,
} from '@features/nvapi-settings';
import { createNvapiProfileContext } from './create-nvapi-profile-context.svelte';

vi.mock('@features/nvapi-settings', () => ({
  createNvapiProfile: vi.fn(),
  deleteNvapiProfile: vi.fn(),
  getNvapiProfileStatus: vi.fn(),
  moveNvapiProfile: vi.fn(),
}));

vi.mock('@shared/error-presentation', () => ({
  formatPresentedError: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));

vi.mock('@shared/errors', () => ({
  reportClientError: vi.fn(),
}));

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
  publishWarningNotification: vi.fn(),
}));

const GAME_ID = 'manual:test-game';
const OTHER_GAME_ID = 'manual:other-game';

function profileStatus(overrides: Partial<NvapiProfileStatus> = {}): NvapiProfileStatus {
  return {
    selectedExecutable: 'C:/Games/Test/game.exe',
    bindingPath: 'C:/Games/Test/game.exe',
    profileName: 'RenderPilot - Test',
    state: 'owned',
    isPredefined: false,
    ownedByThisGame: true,
    canCreate: false,
    canDelete: true,
    pendingOperation: null,
    pendingOperationGameId: null,
    detail: null,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function activateGame(
  context: ReturnType<typeof createNvapiProfileContext>,
  gameId = GAME_ID,
  status = profileStatus(),
): Promise<void> {
  vi.mocked(getNvapiProfileStatus).mockResolvedValueOnce(status);
  await context.reload(gameId);
}

describe('createNvapiProfileContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('keeps a committed profile change successful when the dependent refresh fails', async () => {
    vi.mocked(createNvapiProfile).mockResolvedValueOnce();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus())
      .mockResolvedValueOnce(profileStatus());
    const onChanged = vi.fn().mockRejectedValueOnce(new Error('game details refresh failed'));
    const context = createNvapiProfileContext(onChanged);
    await context.reload(GAME_ID);

    await expect(context.create(GAME_ID)).resolves.toBe(true);

    expect(createNvapiProfile).toHaveBeenCalledWith(GAME_ID);
    expect(onChanged).toHaveBeenCalledWith(GAME_ID);
    expect(context.status).toEqual(profileStatus());
    expect(context.loadError).toBeNull();
    expect(context.actionError).toBeNull();
    expect(context.refreshError).toBe(
      'The NVIDIA profile change succeeded, but game details could not be refreshed.',
    );
    expect(reportClientError).toHaveBeenCalledWith('nvapi_profile_refresh', expect.any(Error));
    expect(publishWarningNotification).toHaveBeenCalledOnce();
    expect(publishWarningNotification).toHaveBeenCalledWith(
      'The NVIDIA profile change succeeded, but game details could not be refreshed.',
      'game details refresh failed',
    );
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
  });

  it('keeps a profile status fetch failure separate from a committed-change refresh warning', async () => {
    vi.mocked(createNvapiProfile).mockResolvedValueOnce();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus())
      .mockRejectedValueOnce(new Error('profile status read failed'));
    const onChanged = vi.fn().mockRejectedValueOnce(new Error('game details refresh failed'));
    const context = createNvapiProfileContext(onChanged);
    await context.reload(GAME_ID);

    await expect(context.create(GAME_ID)).resolves.toBe(true);

    expect(context.status).toBeNull();
    expect(context.loadError).toBe('profile status read failed');
    expect(context.actionError).toBeNull();
    expect(context.refreshError).toBe(
      'The NVIDIA profile change succeeded, but game details could not be refreshed.',
    );
    expect(publishWarningNotification).toHaveBeenCalledOnce();
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
  });

  it('keeps the action guard active through status reload and dependent refresh', async () => {
    vi.mocked(createNvapiProfile).mockResolvedValueOnce();
    const statusReload = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus())
      .mockReturnValueOnce(statusReload.promise);
    const dependentRefresh = deferred<undefined>();
    const onChanged = vi.fn(() => dependentRefresh.promise);
    const context = createNvapiProfileContext(onChanged);
    await context.reload(GAME_ID);

    const create = context.create(GAME_ID);
    await vi.waitFor(() => {
      expect(getNvapiProfileStatus).toHaveBeenCalledTimes(2);
    });

    expect(context.busy).toBe(true);
    await expect(context.create(GAME_ID)).resolves.toBe(false);
    expect(createNvapiProfile).toHaveBeenCalledOnce();

    statusReload.resolve(profileStatus());
    await vi.waitFor(() => {
      expect(onChanged).toHaveBeenCalledOnce();
    });

    expect(context.busy).toBe(true);
    await expect(context.remove(GAME_ID)).resolves.toBe(false);
    expect(context.busy).toBe(true);

    dependentRefresh.resolve(undefined);
    await expect(create).resolves.toBe(true);
    expect(context.busy).toBe(false);
  });

  it('keeps the latest same-game reload result when requests resolve out of order', async () => {
    const outdatedStatus = deferred<NvapiProfileStatus>();
    const latestStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus)
      .mockReturnValueOnce(outdatedStatus.promise)
      .mockReturnValueOnce(latestStatus.promise);
    const context = createNvapiProfileContext();

    const outdatedReload = context.reload(GAME_ID);
    const latestReload = context.reload(GAME_ID);
    expect(context.loading).toBe(true);

    latestStatus.resolve(profileStatus({ profileName: 'Latest profile' }));
    await latestReload;
    expect(context.loading).toBe(false);
    expect(context.status?.profileName).toBe('Latest profile');

    outdatedStatus.resolve(profileStatus({ profileName: 'Outdated profile' }));
    await outdatedReload;
    expect(context.loading).toBe(false);
    expect(context.status?.profileName).toBe('Latest profile');
  });

  it('hides the owned binding while loading and drops the previous game profile on a game switch', async () => {
    vi.mocked(getNvapiProfileStatus).mockResolvedValueOnce(profileStatus());
    const context = createNvapiProfileContext();
    await context.reload(GAME_ID);
    expect(context.ownedBindingPath).toBe('C:/Games/Test/game.exe');

    const sameGameStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus).mockReturnValueOnce(sameGameStatus.promise);
    const sameGameReload = context.reload(GAME_ID);
    expect(context.ownedBindingPath).toBeNull();
    sameGameStatus.resolve(profileStatus());
    await sameGameReload;
    expect(context.ownedBindingPath).toBe('C:/Games/Test/game.exe');

    const otherGameStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus).mockReturnValueOnce(otherGameStatus.promise);
    const otherGameReload = context.reload('manual:other-game');
    expect(context.status).toBeNull();
    expect(context.ownedBindingPath).toBeNull();

    otherGameStatus.resolve(profileStatus({ bindingPath: 'C:/Games/Other/game.exe' }));
    await otherGameReload;
    expect(context.ownedBindingPath).toBe('C:/Games/Other/game.exe');
  });

  it('does not reload or refresh the old game after an action crosses a game switch', async () => {
    const action = deferred<undefined>();
    vi.mocked(createNvapiProfile).mockReturnValueOnce(action.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus({ profileName: 'Old game profile' }))
      .mockResolvedValueOnce(profileStatus({ profileName: 'New game profile' }));
    const onChanged = vi.fn();
    const context = createNvapiProfileContext(onChanged);

    await context.reload(GAME_ID);
    const create = context.create(GAME_ID);
    await vi.waitFor(() => {
      expect(createNvapiProfile).toHaveBeenCalledOnce();
    });

    await context.reload('manual:other-game');
    action.resolve(undefined);
    await expect(create).resolves.toBe(true);

    expect(context.status?.profileName).toBe('New game profile');
    expect(getNvapiProfileStatus).toHaveBeenCalledTimes(2);
    expect(onChanged).not.toHaveBeenCalled();
    expect(context.busy).toBe(false);
  });

  it.each(['switch', 'clear'] as const)(
    'refreshes a committed move after returning to game A through %s while retaining the mutation fence',
    async (transition) => {
      const action = deferred<undefined>();
      const dependentRefresh = deferred<undefined>();
      vi.mocked(moveNvapiProfile).mockReturnValueOnce(action.promise);
      const statusReads = [
        profileStatus({ profileName: 'Initial A profile' }),
        ...(transition === 'switch' ? [profileStatus({ profileName: 'Game B profile' })] : []),
        profileStatus({ profileName: 'A before move commit' }),
        profileStatus({ profileName: 'A after move commit' }),
      ];
      vi.mocked(getNvapiProfileStatus).mockImplementation(() =>
        Promise.resolve(statusReads.shift() ?? profileStatus()),
      );
      const onChanged = vi.fn(() => dependentRefresh.promise);
      const context = createNvapiProfileContext(onChanged);

      await context.reload(GAME_ID);
      const move = context.moveTo(GAME_ID, 'C:/Games/Other/game.exe', true);
      await vi.waitFor(() => {
        expect(moveNvapiProfile).toHaveBeenCalledOnce();
      });

      if (transition === 'switch') {
        await context.reload(OTHER_GAME_ID);
      } else {
        context.clear();
      }
      const returnToA = context.reload(GAME_ID);
      await returnToA;
      expect(context.status?.profileName).toBe('A before move commit');
      expect(context.busy).toBe(true);
      await expect(context.create(GAME_ID)).resolves.toBe(false);
      expect(createNvapiProfile).not.toHaveBeenCalled();

      action.resolve(undefined);
      await vi.waitFor(() => {
        expect(onChanged).toHaveBeenCalledOnce();
      });
      expect(context.status?.profileName).toBe('A after move commit');
      expect(context.busy).toBe(true);
      await expect(context.moveTo(GAME_ID, 'C:/Games/Other/another.exe', false)).resolves.toBe(
        false,
      );

      dependentRefresh.resolve(undefined);
      await expect(move).resolves.toBe(true);
      expect(context.busy).toBe(false);
      expect(onChanged).toHaveBeenCalledWith(GAME_ID);
      expect(getNvapiProfileStatus).toHaveBeenCalledTimes(
        statusReads.length + (transition === 'switch' ? 4 : 3),
      );
    },
  );

  it('suppresses a failed old action notification and status refresh while another game is active', async () => {
    const action = deferred<undefined>();
    vi.mocked(moveNvapiProfile).mockReturnValueOnce(action.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus({ profileName: 'Game A profile' }))
      .mockResolvedValueOnce(profileStatus({ profileName: 'Game B profile' }));
    const onChanged = vi.fn();
    const context = createNvapiProfileContext(onChanged);

    await context.reload(GAME_ID);
    const move = context.moveTo(GAME_ID, 'C:/Games/Other/game.exe', true);
    await vi.waitFor(() => {
      expect(moveNvapiProfile).toHaveBeenCalledOnce();
    });
    await context.reload(OTHER_GAME_ID);

    action.reject(new Error('stale move failed'));
    await expect(move).resolves.toBe(false);

    expect(context.status?.profileName).toBe('Game B profile');
    expect(context.busy).toBe(false);
    expect(getNvapiProfileStatus).toHaveBeenCalledTimes(2);
    expect(onChanged).not.toHaveBeenCalled();
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
    expect(reportClientError).not.toHaveBeenCalled();
  });

  it('rejects a mutation for a game other than the active game', async () => {
    const context = createNvapiProfileContext();
    await activateGame(context);

    await expect(context.create(OTHER_GAME_ID)).resolves.toBe(false);

    expect(createNvapiProfile).not.toHaveBeenCalled();
    expect(context.busy).toBe(false);
    expect(context.status).toEqual(profileStatus());
  });

  it('rejects profile mutations while the executable selection is pending', async () => {
    const context = createNvapiProfileContext(undefined, () => false);
    await activateGame(context);

    await expect(context.create(GAME_ID)).resolves.toBe(false);

    expect(createNvapiProfile).not.toHaveBeenCalled();
    expect(context.busy).toBe(false);
  });

  it('rejects mutations after the active game is cleared', async () => {
    const context = createNvapiProfileContext();
    await activateGame(context);
    context.clear();

    await expect(context.create(GAME_ID)).resolves.toBe(false);

    expect(createNvapiProfile).not.toHaveBeenCalled();
    expect(context.busy).toBe(false);
    expect(context.status).toBeNull();
  });

  it('keeps the profile mutation fence across clear and blocks a second game action', async () => {
    const oldAction = deferred<undefined>();
    vi.mocked(createNvapiProfile).mockReturnValueOnce(oldAction.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus({ profileName: 'Old game profile' }))
      .mockResolvedValueOnce(profileStatus({ profileName: 'New game profile' }));
    const context = createNvapiProfileContext();

    await context.reload(GAME_ID);
    const oldMutation = context.create(GAME_ID);
    await vi.waitFor(() => {
      expect(createNvapiProfile).toHaveBeenCalledOnce();
    });

    context.clear();
    await context.reload(OTHER_GAME_ID);
    expect(context.busy).toBe(true);
    await expect(context.create(OTHER_GAME_ID)).resolves.toBe(false);
    expect(createNvapiProfile).toHaveBeenCalledOnce();

    oldAction.resolve(undefined);
    await expect(oldMutation).resolves.toBe(true);
    expect(context.busy).toBe(false);
    expect(context.status?.profileName).toBe('New game profile');
  });
});
