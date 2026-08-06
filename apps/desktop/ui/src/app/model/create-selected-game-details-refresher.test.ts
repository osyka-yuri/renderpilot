import { describe, expect, it, vi } from 'vitest';

import type { SelectedWorkspaceTarget } from '@app/navigation/selection';
import { createGameDetails } from '@entities/game';

import { createSelectedGameDetailsRefresher } from './create-selected-game-details-refresher';

const DETAILS_TARGET: SelectedWorkspaceTarget = { gameId: 'game-1', screen: 'details' };

describe('createSelectedGameDetailsRefresher', () => {
  it('presents the latest details on the workspace tab resolved at commit time', async () => {
    const pending = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const presentGameDetails = vi.fn();
    let currentTarget: SelectedWorkspaceTarget | null = DETAILS_TARGET;
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: vi.fn(() => pending.promise),
      resolveCurrentTarget: () => currentTarget,
      presentGameDetails,
    });

    const refresh = refresher.refresh(DETAILS_TARGET);
    currentTarget = { ...DETAILS_TARGET, screen: 'operations' };
    const details = createGameDetails();
    pending.resolve(details);
    await refresh;

    expect(presentGameDetails).toHaveBeenCalledWith(details, currentTarget);
  });

  it('lets the latest overlapping refresh own presentation and active errors', async () => {
    const first = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const second = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const presentGameDetails = vi.fn();
    const getDetails = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: getDetails,
      resolveCurrentTarget: () => DETAILS_TARGET,
      presentGameDetails,
    });

    const firstRefresh = refresher.refresh(DETAILS_TARGET);
    const secondRefresh = refresher.refresh(DETAILS_TARGET);
    first.reject(new Error('stale failure'));
    const details = createGameDetails();
    second.resolve(details);

    await expect(firstRefresh).resolves.toBeUndefined();
    await expect(secondRefresh).resolves.toBeUndefined();
    expect(presentGameDetails).toHaveBeenCalledOnce();
    expect(presentGameDetails).toHaveBeenCalledWith(details, DETAILS_TARGET);
  });

  it('suppresses stale success from an overlapping refresh', async () => {
    const first = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const second = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const presentGameDetails = vi.fn();
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise),
      resolveCurrentTarget: () => DETAILS_TARGET,
      presentGameDetails,
    });

    const firstRefresh = refresher.refresh(DETAILS_TARGET);
    const secondRefresh = refresher.refresh(DETAILS_TARGET);
    const staleDetails = createGameDetails({
      game: { identity: { id: 'game-1', title: 'Stale', launcher: 'Manual' } },
    });
    const currentDetails = createGameDetails({
      game: { identity: { id: 'game-1', title: 'Current', launcher: 'Manual' } },
    });
    first.resolve(staleDetails);
    second.resolve(currentDetails);

    await Promise.all([firstRefresh, secondRefresh]);
    expect(presentGameDetails).toHaveBeenCalledOnce();
    expect(presentGameDetails).toHaveBeenCalledWith(currentDetails, DETAILS_TARGET);
  });

  it('propagates an active refresh failure to its route handler', async () => {
    const failure = new Error('details unavailable');
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: vi.fn(() => Promise.reject(failure)),
      resolveCurrentTarget: () => DETAILS_TARGET,
      presentGameDetails: vi.fn(),
    });

    await expect(refresher.refresh(DETAILS_TARGET)).rejects.toBe(failure);
  });

  it('cancel suppresses the current refresh but allows a later one', async () => {
    const pending = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const presentGameDetails = vi.fn();
    const getDetails = vi
      .fn()
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(createGameDetails());
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: getDetails,
      resolveCurrentTarget: () => DETAILS_TARGET,
      presentGameDetails,
    });

    const cancelledRefresh = refresher.refresh(DETAILS_TARGET);
    refresher.cancel();
    pending.reject(new Error('cancelled failure'));
    await expect(cancelledRefresh).resolves.toBeUndefined();

    await refresher.refresh(DETAILS_TARGET);
    expect(presentGameDetails).toHaveBeenCalledOnce();
  });

  it('suppresses presentation and errors after the target leaves the workspace', async () => {
    const pending = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    let currentTarget: SelectedWorkspaceTarget | null = DETAILS_TARGET;
    const presentGameDetails = vi.fn();
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: vi.fn(() => pending.promise),
      resolveCurrentTarget: () => currentTarget,
      presentGameDetails,
    });

    const refresh = refresher.refresh(DETAILS_TARGET);
    currentTarget = null;
    pending.reject(new Error('hidden failure'));

    await expect(refresh).resolves.toBeUndefined();
    expect(presentGameDetails).not.toHaveBeenCalled();
  });

  it('dispose is terminal and suppresses current and future work', async () => {
    const pending = Promise.withResolvers<ReturnType<typeof createGameDetails>>();
    const getDetails = vi.fn(() => pending.promise);
    const presentGameDetails = vi.fn();
    const refresher = createSelectedGameDetailsRefresher({
      getGameDetails: getDetails,
      resolveCurrentTarget: () => DETAILS_TARGET,
      presentGameDetails,
    });

    const refresh = refresher.refresh(DETAILS_TARGET);
    refresher.dispose();
    pending.reject(new Error('disposed failure'));

    await expect(refresh).resolves.toBeUndefined();
    await expect(refresher.refresh(DETAILS_TARGET)).resolves.toBeUndefined();
    expect(getDetails).toHaveBeenCalledOnce();
    expect(presentGameDetails).not.toHaveBeenCalled();
  });
});
