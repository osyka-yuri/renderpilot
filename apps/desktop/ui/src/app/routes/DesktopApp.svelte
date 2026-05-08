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
    scanAutoLibraries,
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

  type ScanError = {
    root: string;
    message: string;
  };

  type ExclusiveTaskOptions = {
    clearErrorOnStart?: boolean;
  };

  const DEFAULT_BACK_TARGET: BackTarget = 'games';

  const STALE_PLAN_MESSAGE =
    'The selected operation plan is no longer current. Rebuild the plan before applying it.';

  const MANUAL_SCAN_DIALOG_OPTIONS = {
    directory: true,
    multiple: false,
    title: 'Select a folder to scan for games',
  } as const;

  let screen: Screen = 'games';
  let backTarget: BackTarget = DEFAULT_BACK_TARGET;

  let games: GameCard[] = [];
  let currentDetails: GameDetails | null = null;
  let selectedGameId: string | null = null;
  let currentPlan: SwapPlan | null = null;

  let busy = false;
  let advancedMode = false;
  let themeMode: ThemeMode = readStoredThemeMode();
  let languageMode: LanguageMode = 'system';
  let errorMessage = '';

  let latestDetailsRequestToken = 0;

  let currentGameCard: GameCard | null;

  $: currentGameCard = selectedGameId ? findGameCard(selectedGameId) : null;

  onMount(() => {
    applyThemeMode(themeMode);

    const stopThemeObserver = observeSystemTheme(() => {
      applyThemeMode(themeMode);
    });

    void startupScanAndRefresh();

    return stopThemeObserver;
  });

  async function startupScanAndRefresh(): Promise<void> {
    await runExclusive(async () => {
      const { errors } = await scanAutoLibraries();

      await refreshGameCards();

      if (errors.length > 0) {
        showPartialScanWarning(errors);
      }
    });
  }

  async function handleScan(): Promise<void> {
    await runExclusive(async () => {
      const selectedFolder = await selectManualScanFolder();

      if (selectedFolder === null) {
        return;
      }

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
    if (!hasCurrentPlan(operationId)) {
      showStalePlanError();
      return;
    }

    await runExclusive(async () => {
      const plan = getCurrentPlanOrThrow(operationId);

      await applyOperationPlan(operationId, plan.confirmation_token);

      currentPlan = null;

      await reloadSelectedGame('details');
    });
  }

  async function handleRollback(operationId: string): Promise<void> {
    await runExclusive(async () => {
      await rollbackOperation(operationId);

      currentPlan = null;

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

    navigateToGames();
  }

  function handleBack(): void {
    if (screen === 'settings') {
      screen = resolveBackTarget();
      return;
    }

    if (screen === 'operations' && hasSelectedGameDetails()) {
      screen = 'details';
      return;
    }

    navigateToGames();
  }

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
      applyThemeMode(mode);

      themeMode = mode;

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

  async function openGame(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    await runExclusive(() => loadGameDetails(gameId, nextScreen));
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    const requestToken = createDetailsRequestToken();
    const details = await getGameDetails(gameId);

    if (!isLatestDetailsRequest(requestToken)) {
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

    clearSelectionIfSelectedGameMissing();
  }

  async function selectManualScanFolder(): Promise<string | null> {
    const selected = await open(MANUAL_SCAN_DIALOG_OPTIONS);

    return typeof selected === 'string' ? selected : null;
  }

  async function runExclusive<T>(
    task: () => Promise<T>,
    options: ExclusiveTaskOptions = {},
  ): Promise<T | null> {
    if (busy) {
      return null;
    }

    const shouldClearError = options.clearErrorOnStart ?? true;

    busy = true;

    if (shouldClearError) {
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

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = details.game.identity.id;

    selectedGameId = gameId;
    currentDetails = details;
    screen = nextScreen;

    if (currentPlan?.game_id !== gameId) {
      currentPlan = null;
    }

    clearError();
  }

  function clearSelectionIfSelectedGameMissing(): void {
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
      navigateToGames();
    }
  }

  function openSettings(): void {
    backTarget = getSettingsBackTarget();
    screen = 'settings';
  }

  function openSelectedWorkspaceScreen(nextScreen: WorkspaceScreen): void {
    if (!hasSelectedGameDetails()) {
      clearSelection();
      return;
    }

    screen = nextScreen;
  }

  function navigateToGames(): void {
    screen = 'games';
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

  function createDetailsRequestToken(): number {
    latestDetailsRequestToken += 1;

    return latestDetailsRequestToken;
  }

  function invalidatePendingGameDetailsRequests(): void {
    createDetailsRequestToken();
  }

  function isLatestDetailsRequest(requestToken: number): boolean {
    return requestToken === latestDetailsRequestToken;
  }

  function getCurrentPlanOrThrow(operationId: string): SwapPlan {
    const plan = currentPlan;

    if (plan !== null && plan.operation_id === operationId) {
      return plan;
    }

    throw new Error(STALE_PLAN_MESSAGE);
  }

  function hasCurrentPlan(operationId: string): boolean {
    return currentPlan?.operation_id === operationId;
  }

  function showStalePlanError(): void {
    showError(new Error(STALE_PLAN_MESSAGE));
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

  function showPartialScanWarning(errors: ScanError[]): void {
    const rootCount = errors.length;
    const rootLabel = rootCount === 1 ? 'root' : 'roots';

    setErrorMessage(
      `Some game libraries could not be scanned (${rootCount} ${rootLabel} failed). Check logs for details.`,
    );
  }

  function restoreThemeMode(mode: ThemeMode): void {
    themeMode = mode;

    ignoreError(() => {
      persistThemeMode(mode);
    });

    ignoreError(() => {
      applyThemeMode(mode);
    });
  }

  function ignoreError(task: () => void): void {
    try {
      task();
    } catch {
      // Preserve the original error.
    }
  }

  function clearError(): void {
    errorMessage = '';
  }

  function setErrorMessage(message: string): void {
    errorMessage = message;
  }

  function showError(error: unknown): void {
    const commandError = normalizeCommandError(error);

    setErrorMessage(describeCommandError(commandError));
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
  {#if screen === 'details' && hasSelectedGameDetails()}
    <GameDetailsScreen
      details={currentDetails}
      gameCard={currentGameCard}
      plan={currentPlan}
      {busy}
      onBuildPlan={handleBuildPlan}
      onApply={handleApply}
      onRollback={handleRollback}
    />
  {:else if screen === 'operations' && hasSelectedGameDetails()}
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
      onRefresh={startupScanAndRefresh}
      onOpenDetails={openGameDetails}
      onOpenOperations={openGameOperations}
    />
  {/if}
</DesktopShell>
