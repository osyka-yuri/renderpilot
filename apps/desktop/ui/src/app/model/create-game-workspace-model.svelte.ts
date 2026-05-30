import { canonicalGameIdentityId } from '@entities/game';
import { isPlanForGame } from '@entities/operation';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import { createRequestChannel } from '@shared/requests';

/**
 * Represents the localized state and behavior of the game workspace.
 * Manages the currently selected game, its details, and the active swap operation plan.
 */
export type GameWorkspaceModel = ReturnType<typeof createGameWorkspaceModel>;

/**
 * Creates an isolated reactive model for the game workspace.
 * Encapsulates the state for the currently active game details and tracks ongoing operations.
 */
export function createGameWorkspaceModel() {
  let selectedGameId = $state<string | null>(null);
  let currentDetails = $state<GameDetails | null>(null);
  let currentPlan = $state<SwapPlan | null>(null);

  const detailsRequests = createRequestChannel();

  /**
   * Updates the workspace state with new game details.
   * If the current operation plan is for a different game, it clears the plan.
   *
   * @param details The newly loaded game details.
   * @returns The canonical game ID if successful, or null if the identity cannot be resolved.
   */
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

  /**
   * Clears the current game selection and cancels any pending details requests.
   */
  function clearSelection(): void {
    detailsRequests.invalidate();

    selectedGameId = null;
    currentDetails = null;
    currentPlan = null;
  }

  /**
   * Sets or clears the active operation plan (e.g., swapping a graphics component).
   *
   * @param plan The new swap plan or null.
   */
  function setCurrentPlan(plan: SwapPlan | null): void {
    currentPlan = plan;
  }

  /**
   * Initiates a new details request cycle, invalidating previous pending requests.
   *
   * @returns A unique token representing the request generation.
   */
  function beginDetailsRequest(): number {
    return detailsRequests.begin();
  }

  /**
   * Checks if a previously generated token is still active (not invalidated).
   *
   * @param token The token returned by `beginDetailsRequest`.
   * @returns true if the request is still active, false otherwise.
   */
  function isDetailsRequestActive(token: number): boolean {
    return detailsRequests.isActive(token);
  }

  /**
   * Retrieves the active plan, but only if it matches the specified game ID.
   *
   * @param gameId The game ID to validate against the current plan.
   * @returns The plan if it's for the requested game, otherwise null.
   */
  function getCurrentPlan(gameId: string): SwapPlan | null {
    if (!isPlanForGame(currentPlan, gameId)) {
      return null;
    }

    return currentPlan;
  }

  return {
    /** The canonically normalized ID of the currently selected game. */
    get selectedGameId() {
      return selectedGameId;
    },
    /** The active details object of the currently selected game. */
    get currentDetails() {
      return currentDetails;
    },
    /** The active component swap operation plan in progress. */
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
