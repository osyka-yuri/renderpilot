<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import { describeCommandError, normalizeCommandError } from '@shared/api/errors';
  import {
    applyThemeMode,
    observeSystemTheme,
    persistThemeMode,
    readStoredThemeMode,
    type ThemeMode,
  } from '@shared/theme/theme-mode';
  import {
    applyOperationPlan,
    buildSwapPlan,
    getGameCards,
    getGameDetails,
    rollbackOperation,
    scanManualFolder,
  } from '@shared/api/desktop';
  import type { GameCard, GameDetails, SwapPlan } from '@shared/api/types';
  import GameDetailsScreen from '@features/game-details/GameDetailsScreen.svelte';
  import GamesScreen from '@features/games/GamesScreen.svelte';
  import OperationsScreen from '@features/operations/OperationsScreen.svelte';
  import SettingsScreen from '@features/settings/SettingsScreen.svelte';
  import type { Screen } from '@app/routes/screen';

  type LanguageMode = 'system' | 'en' | 'ru';
  type BackTarget = 'games' | 'details' | 'operations';

  let screen: Screen = 'games';
  let games: GameCard[] = [];
  let currentDetails: GameDetails | null = null;
  let selectedGameId: string | null = null;
  let currentPlan: SwapPlan | null = null;
  let busy = false;
  let advancedMode = false;
  let themeMode: ThemeMode = readStoredThemeMode();
  let languageMode: LanguageMode = 'system';
  let errorMessage = '';
  let backTarget: BackTarget = 'games';

  $: currentGameCard =
    selectedGameId !== null
      ? games.find((game) => game.game_id === selectedGameId) ?? null
      : null;

  onMount(() => {
    applyThemeMode(themeMode);

    const stopThemeObserver = observeSystemTheme(() => {
      applyThemeMode(themeMode);
    });

    void refreshGames();

    return () => {
      stopThemeObserver();
    };
  });

  async function loadGameDetails(gameId: string, nextScreen: Screen): Promise<void> {
    const details = await getGameDetails(gameId);
    presentGameDetails(details, nextScreen);
  }

  async function refreshGames(): Promise<void> {
    try {
      games = await getGameCards();

      if (selectedGameId && !hasGameCard(selectedGameId)) {
        clearSelection();
      }
    } catch (error) {
      showError(error);
    }
  }

  async function handleScan(): Promise<void> {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select a folder to scan for games',
    });

    if (typeof selected !== 'string') {
      return;
    }

    await withBusy(async () => {
      await scanManualFolder(selected);
      await refreshGames();
    });
  }

  async function openGame(gameId: string, nextScreen: Screen = 'details'): Promise<void> {
    await withBusy(() => loadGameDetails(gameId, nextScreen));
  }

  async function openGameDetails(gameId: string): Promise<void> {
    await openGame(gameId, 'details');
  }

  async function openGameOperations(gameId: string): Promise<void> {
    await openGame(gameId, 'operations');
  }

  async function handleBuildPlan(componentId: string, artifactId: string): Promise<void> {
    const gameId = selectedGameId;

    if (!gameId) {
      return;
    }

    await withBusy(async () => {
      currentPlan = await buildSwapPlan(gameId, componentId, artifactId);
      clearError();
    });
  }

  async function handleApply(operationId: string): Promise<void> {
    const plan = currentPlan;

    if (!plan || plan.operation_id !== operationId) {
      showError(new Error('The selected operation plan is no longer current. Rebuild the plan before applying it.'));
      return;
    }

    await withBusy(async () => {
      await applyOperationPlan(operationId, plan.confirmation_token);
      clearError();
      currentPlan = null;
      await reloadSelectedGame('details');
    });
  }

  async function handleRollback(operationId: string): Promise<void> {
    await withBusy(async () => {
      await rollbackOperation(operationId);
      clearError();
      await reloadSelectedGame(screen === 'operations' ? 'operations' : 'details');
    });
  }

  async function reloadSelectedGame(nextScreen: Screen): Promise<void> {
    const gameId = selectedGameId;

    if (!gameId) {
      return;
    }

    await loadGameDetails(gameId, nextScreen);
  }

  async function withBusy(task: () => Promise<void>): Promise<void> {
    busy = true;

    try {
      await task();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  function presentGameDetails(details: GameDetails, nextScreen: Screen): void {
    currentDetails = details;
    selectedGameId = details.game.identity.id;

    if (currentPlan?.game_id !== details.game.identity.id) {
      currentPlan = null;
    }

    screen = nextScreen;
    clearError();
  }

  function clearSelection(): void {
    selectedGameId = null;
    currentDetails = null;
    currentPlan = null;

    if (isGameWorkspaceScreen(screen)) {
      screen = 'games';
    }
  }

  function clearError(): void {
    errorMessage = '';
  }

  function showError(error: unknown): void {
    const commandError = normalizeCommandError(error);

    errorMessage = describeCommandError(commandError);
  }

  function handleNavigate(nextScreen: Screen): void {
    switch (nextScreen) {
      case 'settings':
        backTarget = isGameWorkspaceScreen(screen) ? screen : 'games';
        screen = 'settings';
        break;
      case 'details':
      case 'operations':
        if (selectedGameId) {
          screen = nextScreen;
        }
        break;
      default:
        screen = 'games';
    }
  }

  function handleBack(): void {
    if (screen === 'settings') {
      screen = backTarget;
      return;
    }

    if (screen === 'operations') {
      screen = 'details';
      return;
    }

    screen = 'games';
  }

  function toggleAdvancedMode(): void {
    advancedMode = !advancedMode;
  }

  function changeThemeMode(mode: ThemeMode): void {
    themeMode = mode;
    persistThemeMode(mode);
    applyThemeMode(mode);
    clearError();
  }

  function changeLanguageMode(mode: LanguageMode): void {
    languageMode = mode;
    clearError();
  }

  function hasGameCard(gameId: string): boolean {
    return games.some((game) => game.game_id === gameId);
  }

  function isGameWorkspaceScreen(value: Screen): value is BackTarget {
    return value === 'details' || value === 'operations';
  }
</script>

<svelte:head>
  <title>RenderPilot Desktop</title>
</svelte:head>

<DesktopShell
  {screen}
  {busy}
  selectedGameTitle={currentGameCard?.title ?? null}
  {errorMessage}
  onNavigate={handleNavigate}
  onBack={handleBack}
>
  {#if screen === 'details'}
    <GameDetailsScreen
      details={currentDetails}
      gameCard={currentGameCard}
      plan={currentPlan}
      {busy}
      onBuildPlan={handleBuildPlan}
      onApply={handleApply}
      onRollback={handleRollback}
    />
  {:else if screen === 'operations'}
    <OperationsScreen
      details={currentDetails}
      gameCard={currentGameCard}
      {busy}
      onRollback={handleRollback}
      onOpenDetails={() => handleNavigate('details')}
    />
  {:else if screen === 'settings'}
    <SettingsScreen
      {themeMode}
      {languageMode}
      {advancedMode}
      onThemeModeChange={changeThemeMode}
      onLanguageModeChange={changeLanguageMode}
      onToggleAdvancedMode={toggleAdvancedMode}
    />
  {:else}
    <GamesScreen
      {games}
      {busy}
      onScan={handleScan}
      onOpenDetails={openGameDetails}
      onOpenOperations={openGameOperations}
    />
  {/if}
</DesktopShell>