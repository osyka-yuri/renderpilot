import type { Screen } from '@app/navigation/screen';
import type { WorkspaceScreen } from '@app/navigation/workspace';
import { isWorkspaceScreen } from '@app/navigation/workspace';
import { resolveSelectedGameDetails, workspaceShellGameTitle } from '@app/navigation/selection';
import type { AppInitializationState } from '@entities/app';
import { findGameSummaryForSelection, gameCardExists } from '@entities/game';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import { ignoreError } from '@shared/callbacks';
import { clearStatusNotification, publishCommandErrorNotification } from '@shared/notifications';
import type { ThemeMode } from '@shared/theme';
import { applyThemeMode, persistThemeMode, readStoredThemeMode } from '@shared/theme';
import type { LanguageMode } from '@shared/i18n';
import { readStoredLanguageMode, setLanguageMode } from '@shared/i18n';
import { createGamesCatalogModel } from '@widgets/games-catalog';
import { createGameWorkspaceModel } from './create-game-workspace-model.svelte';
import { createExclusiveTaskRunner } from '@shared/concurrency';
import {
  publishMissingStableGameDetailsNotification,
  publishStalePlanNotification,
} from './notifications';

export type DesktopAppModel = ReturnType<typeof createDesktopAppModel>;

export type RunExclusiveOptions = {
  clearErrorOnStart?: boolean;
};

/**
 * Safe initialization snapshot for tests and any code path that runs
 * before the Tauri shell has booted. Real prod calls always pass an
 * explicit snapshot fetched in bootstrap.ts.
 */
const DEFAULT_INITIALIZATION: AppInitializationState = {
  isElevated: true,
  elevationSupported: false,
  elevationUserDeclined: false,
  elevationAttempted: false,
};

export function createDesktopAppModel(
  getInitialization: () => AppInitializationState = () => DEFAULT_INITIALIZATION,
) {
  const initialization = getInitialization();
  let screen = $state<Screen>('games');

  const catalog = createGamesCatalogModel();
  const workspace = createGameWorkspaceModel();

  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  let languageMode = $state<LanguageMode>(readStoredLanguageMode());

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

  function getCurrentPlan(gameId: string): SwapPlan | null {
    return workspace.getCurrentPlan(gameId);
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

    const previousMode = languageMode;

    try {
      setLanguageMode(mode);

      languageMode = mode;

      clearError();
    } catch (error) {
      restoreLanguageMode(previousMode);
      showError(error);
    }
  }

  function restoreLanguageMode(mode: LanguageMode): void {
    languageMode = mode;

    ignoreError(() => {
      setLanguageMode(mode);
    });
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

  const taskRunner = createExclusiveTaskRunner({
    onBeforeRun: clearError,
    onError: showError,
  });

  async function runExclusive<T>(
    task: () => Promise<T>,
    options: RunExclusiveOptions = {},
  ): Promise<T | null> {
    return taskRunner.run(
      task,
      options.clearErrorOnStart === false ? { onBeforeRun: undefined } : {},
    );
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
      return taskRunner.busy;
    },
    get themeMode() {
      return themeMode;
    },
    get languageMode() {
      return languageMode;
    },
    // Process-wide initialization snapshot (elevation, etc.). Session-stable.
    get isElevated() {
      return initialization.isElevated;
    },
    get elevationSupported() {
      return initialization.elevationSupported;
    },
    get elevationUserDeclined() {
      return initialization.elevationUserDeclined;
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
