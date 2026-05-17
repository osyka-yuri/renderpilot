import type { Screen } from '@app/navigation/screen';
import type { WorkspaceScreen } from '@app/navigation/workspace';
import { isWorkspaceScreen } from '@app/navigation/workspace';
import { resolveSelectedGameDetails, workspaceShellGameTitle } from '@app/navigation/selection';
import { findGameSummaryForSelection, gameCardExists } from '@entities/game';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import type { LanguageMode } from '@entities/settings';
import { ignoreError } from '@shared/callbacks';
import { clearStatusNotification, publishCommandErrorNotification } from '@shared/notifications';
import type { ThemeMode } from '@shared/theme';
import { applyThemeMode, persistThemeMode, readStoredThemeMode } from '@shared/theme';
import { createGamesCatalogModel } from '@widgets/games-catalog';
import { createGameWorkspaceModel } from '@widgets/game-workspace';
import {
  publishMissingStableGameDetailsNotification,
  publishStalePlanNotification,
} from './notifications';

export type DesktopAppModel = ReturnType<typeof createDesktopAppModel>;

export type RunExclusiveOptions = {
  clearErrorOnStart?: boolean;
};

export function createDesktopAppModel() {
  let screen = $state<Screen>('games');

  const catalog = createGamesCatalogModel();
  const workspace = createGameWorkspaceModel();

  let busy = $state(false);
  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  let languageMode = $state<LanguageMode>('system');

  const currentGameCard = $derived(
    findGameSummaryForSelection(workspace.selectedGameId, catalog.games),
  );
  const selectedDetails = $derived(
    resolveSelectedGameDetails({
      activeScreen: screen,
      selectedGameId: workspace.selectedGameId,
      currentDetails: workspace.currentDetails,
    }),
  );
  const selectedShellGameTitle = $derived(
    workspaceShellGameTitle(currentGameCard, selectedDetails),
  );
  const hasSelectedGameDetails = $derived(selectedDetails !== null);

  // ---------------------------------------------------------------------------
  // Navigation
  // ---------------------------------------------------------------------------

  function handleNavigate(nextScreen: Screen): void {
    if (nextScreen === 'settings') {
      screen = 'settings';
      return;
    }

    if (nextScreen === 'libraries') {
      screen = 'libraries';
      return;
    }

    if (isWorkspaceScreen(nextScreen)) {
      if (!hasSelectedGameDetails) {
        clearSelection();
        return;
      }

      screen = nextScreen;
      return;
    }

    screen = 'games';
  }

  // ---------------------------------------------------------------------------
  // Selection helpers
  // ---------------------------------------------------------------------------

  function clearSelection(): void {
    workspace.clearSelection();

    if (isWorkspaceScreen(screen)) {
      screen = 'games';
    }
  }

  function clearSelectionIfSelectedGameMissing(): void {
    if (
      workspace.selectedGameId === null ||
      gameCardExists(catalog.games, workspace.selectedGameId)
    ) {
      return;
    }

    clearSelection();
  }

  // ---------------------------------------------------------------------------
  // State setters (encapsulated)
  // ---------------------------------------------------------------------------

  function setCurrentPlan(plan: SwapPlan | null): void {
    workspace.setCurrentPlan(plan);
  }

  function getCurrentPlan(operationId: string): SwapPlan | null {
    return workspace.getCurrentPlan(operationId);
  }

  function showStalePlanError(): void {
    publishStalePlanNotification();
  }

  // ---------------------------------------------------------------------------
  // Presenters
  // ---------------------------------------------------------------------------

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = workspace.presentGameDetails(details);

    if (gameId === null) {
      publishMissingStableGameDetailsNotification();
      return;
    }

    screen = nextScreen;

    clearError();
  }

  // ---------------------------------------------------------------------------
  // Error helpers
  // ---------------------------------------------------------------------------

  function clearError(): void {
    clearStatusNotification();
  }

  function showError(error: unknown): void {
    publishCommandErrorNotification(error);
  }

  // ---------------------------------------------------------------------------
  // Settings
  // ---------------------------------------------------------------------------

  function changeThemeMode(mode: ThemeMode): void {
    if (themeMode === mode) {
      return;
    }

    const previousMode = themeMode;

    try {
      persistThemeMode(mode);

      themeMode = mode;
      applyCurrentTheme();

      clearError();
    } catch (error) {
      restoreThemeMode(previousMode);
      showError(error);
    }
  }

  function changeLanguageMode(mode: LanguageMode): void {
    if (languageMode === mode) {
      return;
    }

    languageMode = mode;

    clearError();
  }

  function applyCurrentTheme(): void {
    applyThemeMode(themeMode);
  }

  function restoreThemeMode(mode: ThemeMode): void {
    themeMode = mode;

    ignoreError(() => {
      persistThemeMode(mode);
    });

    ignoreError(() => {
      applyCurrentTheme();
    });
  }

  // ---------------------------------------------------------------------------
  // Exclusive task runner
  // ---------------------------------------------------------------------------

  async function runExclusive<T>(
    task: () => Promise<T>,
    options: RunExclusiveOptions = {},
  ): Promise<T | null> {
    if (busy) {
      return null;
    }

    busy = true;

    if (options.clearErrorOnStart ?? true) {
      clearError();
    }

    try {
      return await task();
    } catch (error) {
      showError(error);
      return null;
    } finally {
      busy = false;
    }
  }

  return {
    // State (read-only)
    get screen() {
      return screen;
    },
    get games() {
      return catalog.games;
    },
    get catalogVersion() {
      return catalog.catalogVersion;
    },
    get selectedGameId() {
      return workspace.selectedGameId;
    },
    get currentDetails() {
      return workspace.currentDetails;
    },
    get currentPlan() {
      return workspace.currentPlan;
    },
    get busy() {
      return busy;
    },
    get themeMode() {
      return themeMode;
    },
    get languageMode() {
      return languageMode;
    },
    // Derived
    get currentGameCard() {
      return currentGameCard;
    },
    get selectedDetails() {
      return selectedDetails;
    },
    get selectedShellGameTitle() {
      return selectedShellGameTitle;
    },
    get hasSelectedGameDetails() {
      return hasSelectedGameDetails;
    },

    // Sub-models (direct access for pass-through operations)
    get catalog() {
      return catalog;
    },
    get workspace() {
      return workspace;
    },

    // State mutations (encapsulated)
    setCurrentPlan,

    // Actions
    handleNavigate,
    clearSelection,
    clearSelectionIfSelectedGameMissing,
    getCurrentPlan,
    showStalePlanError,
    presentGameDetails,
    clearError,
    showError,
    changeThemeMode,
    changeLanguageMode,
    applyCurrentTheme,
    runExclusive,
  };
}
