import type { SelectedWorkspaceTarget } from '@app/navigation/selection';
import { getGameDetails, type GameDetails } from '@entities/game';
import { createRequestChannel } from '@shared/requests';

export type SelectedGameDetailsRefresherDeps = {
  getGameDetails?: typeof getGameDetails;
  resolveCurrentTarget: (gameId: string) => SelectedWorkspaceTarget | null;
  presentGameDetails: (details: GameDetails, target: SelectedWorkspaceTarget) => void;
};

/** Passive selected-game refresh that never owns foreground navigation. */
export function createSelectedGameDetailsRefresher(deps: SelectedGameDetailsRefresherDeps) {
  const requests = createRequestChannel();
  let disposed = false;

  function resolveRelevantTarget(
    requestId: number,
    gameId: string,
  ): SelectedWorkspaceTarget | null {
    if (disposed || !requests.isActive(requestId)) {
      return null;
    }
    return deps.resolveCurrentTarget(gameId);
  }

  async function refresh(target: SelectedWorkspaceTarget): Promise<void> {
    if (disposed) {
      return;
    }

    const requestId = requests.begin();
    let details: GameDetails;
    try {
      details = await (deps.getGameDetails ?? getGameDetails)(target.gameId);
    } catch (error) {
      if (resolveRelevantTarget(requestId, target.gameId) === null) {
        return;
      }
      throw error;
    }

    const currentTarget = resolveRelevantTarget(requestId, target.gameId);
    if (currentTarget !== null) {
      deps.presentGameDetails(details, currentTarget);
    }
  }

  function cancel(): void {
    if (!disposed) {
      requests.invalidate();
    }
  }

  function dispose(): void {
    if (disposed) {
      return;
    }
    disposed = true;
    requests.invalidate();
  }

  return { refresh, cancel, dispose };
}
