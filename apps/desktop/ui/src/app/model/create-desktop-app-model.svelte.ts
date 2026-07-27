import type { Screen } from '@app/navigation/screen';
import type { WorkspaceScreen } from '@app/navigation/workspace';
import { isWorkspaceScreen } from '@app/navigation/workspace';
import { resolveSelectedGameDetails } from '@app/navigation/selection';
import type { AppInitializationState } from '@entities/app';
import type { GameDetails } from '@entities/game';
import { ignoreError } from '@shared/callbacks';
import { clearStatusNotification, publishCommandErrorNotification } from '@shared/notifications';
import type { ThemeMode } from '@shared/theme';
import { applyThemeMode, persistThemeMode, readStoredThemeMode } from '@shared/theme';
import type { LanguageMode } from '@shared/i18n';
import { readStoredLanguageMode, setLanguageMode } from '@shared/i18n';
import { createGameWorkspaceModel } from './create-game-workspace-model.svelte';
import { createExclusiveTaskRunner } from '@shared/concurrency';
import { publishMissingStableGameDetailsNotification } from './notifications';

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

/**
 * Root desktop application model.
 *
 * Public surface style:
 * - **Flat getters** for template-friendly reads (screen, busy, theme, …)
 * - **Nested `workspace`** for details mutations and request tokens.
 *
 * Plan APIs live only on `workspace` — the root model does not re-wrap them.
 */
export function createDesktopAppModel(
  getInitialization: () => AppInitializationState = () => DEFAULT_INITIALIZATION,
) {
  const initialization = getInitialization();
  let screen = $state<Screen>('games');

  const workspace = createGameWorkspaceModel();

  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  let languageMode = $state<LanguageMode>(readStoredLanguageMode());

  const selectedDetails = $derived(
    resolveSelectedGameDetails({
      activeScreen: screen,
      selectedGameId: workspace.selectedGameId,
      currentDetails: workspace.currentDetails,
    }),
  );
  const hasSelectedGameDetails = $derived(selectedDetails !== null);

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

  function clearSelection(): void {
    workspace.clearSelection();

    if (isWorkspaceScreen(screen)) {
      screen = 'games';
    }
  }

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = workspace.presentGameDetails(details);

    if (gameId === null) {
      publishMissingStableGameDetailsNotification();
      return;
    }

    screen = nextScreen;
    clearError();
  }

  function clearError(): void {
    clearStatusNotification();
  }

  function showError(error: unknown): void {
    publishCommandErrorNotification(error);
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
    // Flat read surface (templates + simple callers)
    get screen() {
      return screen;
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
    get isElevated() {
      return initialization.isElevated;
    },
    get elevationSupported() {
      return initialization.elevationSupported;
    },
    get elevationUserDeclined() {
      return initialization.elevationUserDeclined;
    },
    get elevationAttempted() {
      return initialization.elevationAttempted;
    },
    get selectedDetails() {
      return selectedDetails;
    },
    get hasSelectedGameDetails() {
      return hasSelectedGameDetails;
    },

    // Nested mutation / request surfaces
    get workspace() {
      return workspace;
    },

    // Root-owned actions
    handleNavigate,
    clearSelection,
    presentGameDetails,
    clearError,
    showError,
    changeThemeMode,
    changeLanguageMode,
    applyCurrentTheme,
    runExclusive,
  };
}
