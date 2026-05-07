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
  type WorkspaceScreen = Extract<Screen, 'details' | 'operations'>;
  type BackTarget = 'games' | WorkspaceScreen;

  const DEFAULT_BACK_TARGET: BackTarget = 'games';

  const MANUAL_SCAN_DIALOG_OPTIONS = {
    directory: true,
    multiple: false,
    title: 'Select a folder to scan for games',
  } as const;

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
  let backTarget: BackTarget = DEFAULT_BACK_TARGET;

  let latestDetailsRequestToken = 0;

  $: currentGameCard = selectedGameId ? findGameCard(selectedGameId) : null;

  onMount(() => {
    applyThemeMode(themeMode);

    const stopThemeObserver = observeSystemTheme(() => {
      applyThemeMode(themeMode);
    });

    void refreshGames();

    return stopThemeObserver;
  });

  async function refreshGames(): Promise<void> {
    try {
      await refreshGameCards();
      clearError();
    } catch (error) {
      showError(error);
    }
  }

  async function handleScan(): Promise<void> {
    const selectedFolder = await selectManualScanFolder();

    if (selectedFolder === null) {
      return;
    }

    await runExclusive(async () => {
      await scanManualFolder(selectedFolder);
      await refreshGameCards();
    });
  }

  async function openGameDetails(gameId: string): Promise<void> {
    await openGame(gameId, 'details');
  }

  async function openGameOperations(gameId: string): Promise<void> {
    await openGame(gameId, 'operations');
  }

  async function handleBuildPlan(componentId: string, artifactId: string): Promise<void> {
    const gameId = selectedGameId;

    if (gameId === null) {
      return;
    }

    await runExclusive(async () => {
      const plan = await buildSwapPlan(gameId, componentId, artifactId);

      if (isSelectedGame(gameId)) {
        currentPlan = plan;
      }
    });
  }

  async function handleApply(operationId: string): Promise<void> {
    const plan = currentPlan;

    if (!isCurrentPlan(plan, operationId)) {
      showError(
        new Error(
          'The selected operation plan is no longer current. Rebuild the plan before applying it.',
        ),
      );
      return;
    }

    await runExclusive(async () => {
      await applyOperationPlan(operationId, plan.confirmation_token);

      currentPlan = null;

      await reloadSelectedGame('details');
    });
  }

  async function handleRollback(operationId: string): Promise<void> {
    await runExclusive(async () => {
      await rollbackOperation(operationId);
      await reloadSelectedGame(getScreenAfterRollback());
    });
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

    screen = 'games';
  }

  function handleBack(): void {
    if (screen === 'settings') {
      screen = resolveBackTarget();
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
    if (themeMode === mode) {
      return;
    }

    try {
      themeMode = mode;
      persistThemeMode(mode);
      applyThemeMode(mode);
      clearError();
    } catch (error) {
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

  async function openGame(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    await runExclusive(() => loadGameDetails(gameId, nextScreen));
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    const requestToken = invalidatePendingGameDetailsRequests();
    const details = await getGameDetails(gameId);

    if (requestToken !== latestDetailsRequestToken) {
      return;
    }

    presentGameDetails(details, nextScreen);
  }

  async function reloadSelectedGame(nextScreen: WorkspaceScreen): Promise<void> {
    const gameId = selectedGameId;

    if (gameId === null) {
      return;
    }

    await loadGameDetails(gameId, nextScreen);
  }

  async function refreshGameCards(): Promise<void> {
    games = await getGameCards();
    clearSelectionIfGameMissing();
  }

  async function selectManualScanFolder(): Promise<string | null> {
    const selected = await open(MANUAL_SCAN_DIALOG_OPTIONS);

    return typeof selected === 'string' ? selected : null;
  }

  async function runExclusive(task: () => Promise<void>): Promise<void> {
    if (busy) {
      return;
    }

    busy = true;

    try {
      await task();
      clearError();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = details.game.identity.id;

    currentDetails = details;
    selectedGameId = gameId;
    screen = nextScreen;

    if (currentPlan?.game_id !== gameId) {
      currentPlan = null;
    }

    clearError();
  }

  function clearSelectionIfGameMissing(): void {
    if (selectedGameId !== null && !hasGameCard(selectedGameId)) {
      clearSelection();
    }
  }

  function clearSelection(): void {
    invalidatePendingGameDetailsRequests();

    selectedGameId = null;
    currentDetails = null;
    currentPlan = null;

    if (isWorkspaceScreen(backTarget)) {
      backTarget = DEFAULT_BACK_TARGET;
    }

    if (isWorkspaceScreen(screen)) {
      screen = 'games';
    }
  }

  function openSettings(): void {
    backTarget = getSettingsBackTarget();
    screen = 'settings';
  }

  function openSelectedWorkspaceScreen(nextScreen: WorkspaceScreen): void {
    if (!hasSelectedGameDetails()) {
      screen = 'games';
      return;
    }

    screen = nextScreen;
  }

  function getSettingsBackTarget(): BackTarget {
    return isWorkspaceScreen(screen) && hasSelectedGameDetails() ? screen : DEFAULT_BACK_TARGET;
  }

  function resolveBackTarget(): BackTarget {
    if (isWorkspaceScreen(backTarget) && !hasSelectedGameDetails()) {
      return DEFAULT_BACK_TARGET;
    }

    return backTarget;
  }

  function getScreenAfterRollback(): WorkspaceScreen {
    return screen === 'operations' ? 'operations' : 'details';
  }

  function invalidatePendingGameDetailsRequests(): number {
    latestDetailsRequestToken += 1;

    return latestDetailsRequestToken;
  }

  function isCurrentPlan(plan: SwapPlan | null, operationId: string): plan is SwapPlan {
    return plan?.operation_id === operationId;
  }

  function isSelectedGame(gameId: string): boolean {
    return selectedGameId === gameId;
  }

  function hasSelectedGameDetails(): boolean {
    return selectedGameId !== null && currentDetails?.game.identity.id === selectedGameId;
  }

  function findGameCard(gameId: string): GameCard | null {
    return games.find((game) => game.game_id === gameId) ?? null;
  }

  function hasGameCard(gameId: string): boolean {
    return findGameCard(gameId) !== null;
  }

  function isWorkspaceScreen(value: Screen | BackTarget): value is WorkspaceScreen {
    return value === 'details' || value === 'operations';
  }

  function clearError(): void {
    errorMessage = '';
  }

  function showError(error: unknown): void {
    const commandError = normalizeCommandError(error);

    errorMessage = describeCommandError(commandError);
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
      onOpenDetails={() => {
        handleNavigate('details');
      }}
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
