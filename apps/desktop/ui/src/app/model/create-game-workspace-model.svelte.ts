import { canonicalGameIdentityId } from '@entities/game';
import { isPlanForGame } from '@entities/operation';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import { createRequestChannel } from '@shared/requests';

export type GameWorkspaceModel = ReturnType<typeof createGameWorkspaceModel>;

export function createGameWorkspaceModel() {
  let selectedGameId = $state<string | null>(null);
  let currentDetails = $state<GameDetails | null>(null);
  let currentPlan = $state<SwapPlan | null>(null);

  const detailsRequests = createRequestChannel();

  function presentGameDetails(details: GameDetails): string | null {
    const gameId = canonicalGameIdentityId(details);

    if (gameId === null) {
      return null;
    }

    selectedGameId = gameId;
    currentDetails = details;

    if (!isPlanForGame(currentPlan, gameId)) {
      currentPlan = null;
    }

    return gameId;
  }

  function clearSelection(): void {
    detailsRequests.invalidate();

    selectedGameId = null;
    currentDetails = null;
    currentPlan = null;
  }

  function setCurrentPlan(plan: SwapPlan | null): void {
    currentPlan = plan;
  }

  function beginDetailsRequest(): number {
    return detailsRequests.begin();
  }

  function isDetailsRequestActive(token: number): boolean {
    return detailsRequests.isActive(token);
  }

  function getCurrentPlan(gameId: string): SwapPlan | null {
    if (!isPlanForGame(currentPlan, gameId)) {
      return null;
    }

    return currentPlan;
  }

  return {
    get selectedGameId() {
      return selectedGameId;
    },
    get currentDetails() {
      return currentDetails;
    },
    get currentPlan() {
      return currentPlan;
    },
    presentGameDetails,
    clearSelection,
    setCurrentPlan,
    beginDetailsRequest,
    isDetailsRequestActive,
    getCurrentPlan,
  };
}
