import type { Screen } from '@app/navigation/screen';
import type { WorkspaceScreen } from '@app/navigation/workspace';
import { isWorkspaceScreen } from '@app/navigation/workspace';
import { resolveSelectedGameDetails } from '@app/navigation/selection';
import { DEFAULT_APP_INITIALIZATION, type AppInitializationState } from '@entities/app';
import type { GameDetails } from '@entities/game';
import { ignoreError } from '@shared/callbacks';
import { clearStatusNotification, publishCommandErrorNotification } from '@shared/notifications';
import type { ThemeMode } from '@shared/theme';
import { applyThemeMode, persistThemeMode, readStoredThemeMode } from '@shared/theme';
import type { LanguageMode } from '@shared/i18n';
import { getI18nState, setLanguageMode } from '@shared/i18n';
import { createExclusiveTaskRunner } from '@shared/concurrency';
import { createGameWorkspaceModel } from './create-game-workspace-model.svelte';
import { publishMissingStableGameDetailsNotification } from './notifications';

export type DesktopAppModel = ReturnType<typeof createDesktopAppModel>;

export type RunExclusiveOptions = {
  clearErrorOnStart?: boolean;
  onError?: (error: unknown) => void;
};

/**
 * Root desktop application model.
 *
 * The default initialization snapshot keeps tests and pre-shell callers safe;
 * production bootstrap passes the explicit backend snapshot.
 *
 * Public surface style:
 * - **Flat getters** for template-friendly reads (screen, busy, theme, …)
 * - **Nested `workspace`** for details mutations and request tokens.
 *
 * Plan APIs live only on `workspace` — the root model does not re-wrap them.
 */
export function createDesktopAppModel(
  getInitialization: () => AppInitializationState = () => DEFAULT_APP_INITIALIZATION,
) {
  const initialization = getInitialization();
  let screen = $state<Screen>('games');

  const workspace = createGameWorkspaceModel();

  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  const i18nState = $derived(getI18nState());

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

  async function changeLanguageMode(mode: LanguageMode): Promise<void> {
    try {
      const result = await setLanguageMode(mode);

      if (result.outcome === 'applied') {
        clearError();
      }
    } catch (error) {
      showError(error);
    }
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
    return taskRunner.run(task, {
      ...(options.clearErrorOnStart === false ? { onBeforeRun: undefined } : {}),
      ...(options.onError === undefined ? {} : { onError: options.onError }),
    });
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
      return i18nState.pending?.mode ?? i18nState.activeMode;
    },
    get languageBusy() {
      return i18nState.status === 'loading';
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
