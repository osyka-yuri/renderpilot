import type { Screen } from '@app/navigation/screen';
import type { BackTarget, WorkspaceScreen } from '@app/navigation/workspace';
import {
  DEFAULT_BACK_TARGET,
  getSettingsBackTarget,
  isWorkspaceScreen,
  resolveBackTarget,
} from '@app/navigation/workspace';
import { resolveSelectedGameDetails, workspaceShellGameTitle } from '@app/navigation/selection';
import { findGameSummaryForSelection, gameCardExists } from '@entities/game';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import type { LanguageMode } from '@entities/settings';
import { describeCommandError } from '@shared/api';
import { ignoreError } from '@shared/utils';
import type { ThemeMode } from '@shared/theme';
import { applyThemeMode, persistThemeMode, readStoredThemeMode } from '@shared/theme';
import { createGamesCatalogModel } from '@widgets/games-catalog';
import { createGameWorkspaceModel } from '@widgets/game-workspace';
import { STALE_PLAN_MESSAGE } from './messages';

export type DesktopAppModel = ReturnType<typeof createDesktopAppModel>;

export type RunExclusiveOptions = {
  clearErrorOnStart?: boolean;
};

export function createDesktopAppModel() {
  let screen = $state<Screen>('games');
  let backTarget = $state<BackTarget>(DEFAULT_BACK_TARGET);

  const catalog = createGamesCatalogModel();
  const workspace = createGameWorkspaceModel({ stalePlanMessage: STALE_PLAN_MESSAGE });

  let busy = $state(false);
  let advancedMode = $state(false);
  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  let languageMode = $state<LanguageMode>('system');
  let errorMessage = $state('');

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

  function openSettings(): void {
    backTarget = getSettingsBackTarget(screen, hasSelectedGameDetails);
    screen = 'settings';
  }

  function openSelectedWorkspaceScreen(nextScreen: WorkspaceScreen): void {
    if (!hasSelectedGameDetails) {
      clearSelection();
      return;
    }

    screen = nextScreen;
  }

  function navigateToGames(): void {
    screen = 'games';
  }

  function handleNavigate(nextScreen: Screen): void {
    if (nextScreen === 'settings') {
      openSettings();
      return;
    }

    if (isWorkspaceScreen(nextScreen)) {
      openSelectedWorkspaceScreen(nextScreen);
      return;
    }

    navigateToGames();
  }

  function handleBack(): void {
    if (screen === 'settings') {
      screen = resolveBackTarget(backTarget, hasSelectedGameDetails);
      return;
    }

    if (screen === 'operations' && hasSelectedGameDetails) {
      screen = 'details';
      return;
    }

    navigateToGames();
  }

  // ---------------------------------------------------------------------------
  // Selection helpers
  // ---------------------------------------------------------------------------

  function clearSelection(): void {
    workspace.clearSelection();

    if (isWorkspaceScreen(backTarget)) {
      backTarget = DEFAULT_BACK_TARGET;
    }

    if (isWorkspaceScreen(screen)) {
      navigateToGames();
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
    showError(workspace.showStalePlanError());
  }

  // ---------------------------------------------------------------------------
  // Presenters
  // ---------------------------------------------------------------------------

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = workspace.presentGameDetails(details);

    if (gameId === null) {
      showError(new Error('Catalog returned game details without a stable identifier.'));
      return;
    }

    screen = nextScreen;

    clearError();
  }

  // ---------------------------------------------------------------------------
  // Error helpers
  // ---------------------------------------------------------------------------

  function clearError(): void {
    errorMessage = '';
  }

  function setErrorMessage(message: string): void {
    errorMessage = message;
  }

  function showError(error: unknown): void {
    setErrorMessage(describeCommandError(error));
  }

  // ---------------------------------------------------------------------------
  // Settings
  // ---------------------------------------------------------------------------

  function toggleAdvancedMode(): void {
    advancedMode = !advancedMode;
  }

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
    get backTarget() {
      return backTarget;
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
    get advancedMode() {
      return advancedMode;
    },
    get themeMode() {
      return themeMode;
    },
    get languageMode() {
      return languageMode;
    },
    get errorMessage() {
      return errorMessage;
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
    handleBack,
    clearSelection,
    clearSelectionIfSelectedGameMissing,
    getCurrentPlan,
    showStalePlanError,
    presentGameDetails,
    clearError,
    setErrorMessage,
    showError,
    toggleAdvancedMode,
    changeThemeMode,
    changeLanguageMode,
    applyCurrentTheme,
    runExclusive,
  };
}
