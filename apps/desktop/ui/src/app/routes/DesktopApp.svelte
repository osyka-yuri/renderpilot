<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { open } from '@tauri-apps/plugin-dialog';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import type { Screen } from '@app/routes/screen';
  import {
    DEFAULT_BACK_TARGET,
    formatPartialScanWarning,
    getScreenAfterRollback,
    getSettingsBackTarget,
    isWorkspaceScreen,
    resolveBackTarget,
    type BackTarget,
    type WorkspaceScreen,
  } from '@app/routes/desktop-app-controller';
  import {
    canonicalGameIdentityId,
    findGameCardForSelection,
    normalizeSelectableGameId,
    resolveSelectedGameDetails,
    workspaceShellGameTitle,
  } from '@app/routes/desktop-selection';
  import {
    describeCommandError,
    describeCommandErrorBrief,
    normalizeCommandError,
  } from '@shared/api/errors';
  import {
    DEFAULT_GAME_CARDS_CATALOG_PAGE,
    DEFAULT_GAME_CARDS_CATALOG_SORT,
  } from '@shared/api/game-cards-defaults';
  import {
    applyOperationPlan,
    buildSwapPlan,
    fetchGameCover,
    getCatalogSetting,
    getGameDetails,
    queryGameCards,
    rollbackOperation,
    scanAutoLibraries,
    scanManualFolder,
    STEAMGRIDDB_SETTING_KEY,
  } from '@shared/api/desktop';
  import type { GameCard, GameDetails, SwapPlan } from '@shared/api/types';
  import {
    applyThemeMode,
    observeSystemTheme,
    persistThemeMode,
    readStoredThemeMode,
    type ThemeMode,
  } from '@shared/theme/theme-mode';
  import {
    combineCoverSyncMessages,
    COVER_FETCH_CONCURRENCY,
    fetchCoverRemotePolicy,
    fetchSteamGridDbKeyConfigured,
    filterGamesMissingStoredCoverForBackgroundSync,
    formatCoverSyncBanner,
    runCoverFetchBatch,
  } from '@shared/covers/cover-sync';
  import { createRequestChannel } from '@shared/utils/request-channel';

  import GameDetailsScreen from '@features/game-details/GameDetailsScreen.svelte';
  import GamesScreen from '@features/games/GamesScreen.svelte';
  import OperationsScreen from '@features/operations/OperationsScreen.svelte';
  import SettingsScreen from '@features/settings/SettingsScreen.svelte';

  type LanguageMode = 'system' | 'en' | 'ru';

  type ScanError = {
    root: string;
    message: string;
  };

  type ExclusiveTaskOptions = {
    clearErrorOnStart?: boolean;
  };

  const STALE_PLAN_MESSAGE =
    'The selected operation plan is no longer current. Rebuild the plan before applying it.';

  const MANUAL_SCAN_DIALOG_OPTIONS = {
    directory: true,
    multiple: false,
    title: 'Select a folder to scan for games',
  } as const;

  let screen = $state<Screen>('games');
  let backTarget = $state<BackTarget>(DEFAULT_BACK_TARGET);

  let games = $state<GameCard[]>([]);
  let catalogVersion = $state(0);
  let selectedGameId = $state<string | null>(null);
  let currentDetails = $state<GameDetails | null>(null);
  let currentPlan = $state<SwapPlan | null>(null);

  let busy = $state(false);
  let advancedMode = $state(false);
  let themeMode = $state<ThemeMode>(readStoredThemeMode());
  let languageMode = $state<LanguageMode>('system');
  let errorMessage = $state('');

  let coversSyncing = $state(false);
  let coverSyncQueued = $state(false);
  const coversAutoFetchingIds = new SvelteSet<string>();

  const detailsRequests = createRequestChannel();

  const currentGameCard = $derived(findGameCardForSelection(selectedGameId, games));
  const selectedDetails = $derived(
    resolveSelectedGameDetails({
      activeScreen: screen,
      selectedGameId,
      currentDetails,
    }),
  );
  const selectedShellGameTitle = $derived(
    workspaceShellGameTitle(currentGameCard, selectedDetails),
  );

  onMount(() => {
    applyCurrentTheme();

    const stopThemeObserver = observeSystemTheme(() => {
      applyCurrentTheme();
    });

    void scanAutoLibrariesAndRefreshCards();

    return stopThemeObserver;
  });

  // ---------------------------------------------------------------------------
  // Catalog loading
  // ---------------------------------------------------------------------------

  async function scanAutoLibrariesAndRefreshCards(): Promise<void> {
    await runCatalogRefreshWithCoverSync(async () => {
      await scanAutoLibrariesSafely();
      return true;
    });
  }

  async function handleScan(): Promise<void> {
    await runCatalogRefreshWithCoverSync(async () => {
      const selectedFolder = await selectManualScanFolder();

      if (selectedFolder === null) {
        return false;
      }

      await scanManualFolder(selectedFolder);
      return true;
    });
  }

  async function handleReloadCards(): Promise<void> {
    await runExclusive(refreshGameCards);
  }

  async function runCatalogRefreshWithCoverSync(
    prepareRefresh: () => Promise<boolean>,
  ): Promise<void> {
    const refreshed = await runExclusive(async () => {
      const shouldRefresh = await prepareRefresh();

      if (!shouldRefresh) {
        return false;
      }

      await refreshGameCards();
      return true;
    });

    if (refreshed === true) {
      queueMissingCoverSync();
    }
  }

  async function scanAutoLibrariesSafely(): Promise<void> {
    try {
      const scanResult = await scanAutoLibraries();
      const scanErrors = scanResult.errors ?? [];

      if (scanErrors.length > 0) {
        showPartialScanWarning(scanErrors);
      }
    } catch (error) {
      setErrorMessage(
        `Automatic library scan failed; your game list was still refreshed. ${describeCommandErrorBrief(
          error,
        )}`,
      );
    }
  }

  async function refreshGameCards(): Promise<void> {
    const result = await queryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
      page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
    });

    games = result.items;
    catalogVersion += 1;

    clearSelectionIfSelectedGameMissing();
  }

  async function selectManualScanFolder(): Promise<string | null> {
    const selected = await open(MANUAL_SCAN_DIALOG_OPTIONS);

    return typeof selected === 'string' ? selected : null;
  }

  // ---------------------------------------------------------------------------
  // Game details / operations
  // ---------------------------------------------------------------------------

  async function openGameDetails(gameId: string): Promise<void> {
    await openGame(gameId, 'details');
  }

  async function openGameOperations(gameId: string): Promise<void> {
    await openGame(gameId, 'operations');
  }

  async function openGame(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    const normalizedGameId = normalizeSelectableGameId(gameId);

    if (normalizedGameId.length === 0) {
      return;
    }

    await runExclusive(() => loadGameDetails(normalizedGameId, nextScreen));
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    const requestToken = detailsRequests.begin();
    const details = await getGameDetails(gameId);

    if (!detailsRequests.isActive(requestToken)) {
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

  function presentGameDetails(details: GameDetails, nextScreen: WorkspaceScreen): void {
    const gameId = canonicalGameIdentityId(details);

    if (gameId === null) {
      showError(new Error('Catalog returned game details without a stable identifier.'));
      return;
    }

    selectedGameId = gameId;
    currentDetails = details;
    screen = nextScreen;

    if (!isPlanForGame(currentPlan, gameId)) {
      currentPlan = null;
    }

    clearError();
  }

  async function handleBuildPlan(componentId: string, artifactId: string): Promise<void> {
    const gameId = selectedGameId;

    if (gameId === null) {
      return;
    }

    await runExclusive(async () => {
      if (isSelectedGame(gameId)) {
        currentPlan = null;
      }

      const plan = await buildSwapPlan(gameId, componentId, artifactId);

      if (isSelectedGame(gameId)) {
        currentPlan = plan;
      }
    });
  }

  async function handleApply(operationId: string): Promise<void> {
    const plan = getCurrentPlan(operationId);

    if (plan === null) {
      showStalePlanError();
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

      currentPlan = null;

      await reloadSelectedGame(getScreenAfterRollback(screen));
    });
  }

  function getCurrentPlan(operationId: string): SwapPlan | null {
    if (currentPlan?.operation_id !== operationId) {
      return null;
    }

    return currentPlan;
  }

  function showStalePlanError(): void {
    showError(new Error(STALE_PLAN_MESSAGE));
  }

  // ---------------------------------------------------------------------------
  // Navigation
  // ---------------------------------------------------------------------------

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
      screen = resolveBackTarget(backTarget, hasSelectedGameDetails());
      return;
    }

    if (screen === 'operations' && hasSelectedGameDetails()) {
      screen = 'details';
      return;
    }

    navigateToGames();
  }

  function openSettings(): void {
    backTarget = getSettingsBackTarget(screen, hasSelectedGameDetails());
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
  // Background cover sync
  // ---------------------------------------------------------------------------

  function queueMissingCoverSync(): void {
    coverSyncQueued = true;

    if (coversSyncing) {
      return;
    }

    void drainMissingCoverSyncQueue().catch(handleBackgroundCoverSyncError);
  }

  async function drainMissingCoverSyncQueue(): Promise<void> {
    if (coversSyncing) {
      return;
    }

    coversSyncing = true;

    try {
      while (coverSyncQueued) {
        coverSyncQueued = false;
        await syncMissingCoversAfterCardsLoad();
      }
    } finally {
      clearCoverAutoFetching();
      coversSyncing = false;
    }
  }

  async function syncMissingCoversAfterCardsLoad(): Promise<void> {
    await tick();

    const cardSnapshot = games.slice();

    if (cardSnapshot.length === 0) {
      return;
    }

    const missingCoverCards = await getMissingStoredCoverCards(cardSnapshot);

    if (missingCoverCards.length === 0) {
      return;
    }

    const { failures } = await runCoverFetchBatch({
      games: missingCoverCards,
      concurrency: COVER_FETCH_CONCURRENCY,
      fetchCover: async (gameId): Promise<void> => {
        await fetchGameCover(gameId);
      },
      onGameStart: (gameId) => {
        setCoverAutoFetching(gameId, true);
      },
      onGameEnd: (gameId) => {
        setCoverAutoFetching(gameId, false);
      },
    });

    const refreshAfterSyncError = await refreshCardsAfterCoverSync();
    const combinedMessage = combineCoverSyncMessages(
      formatCoverSyncBanner(failures),
      refreshAfterSyncError,
    );

    if (combinedMessage !== null) {
      setErrorMessage(combinedMessage);
    }
  }

  async function getMissingStoredCoverCards(cardSnapshot: GameCard[]): Promise<GameCard[]> {
    const [policy, hasSteamGridDbApiKey] = await Promise.all([
      fetchCoverRemotePolicy(getCatalogSetting),
      fetchSteamGridDbKeyConfigured(getCatalogSetting, STEAMGRIDDB_SETTING_KEY),
    ]);

    return filterGamesMissingStoredCoverForBackgroundSync(
      cardSnapshot,
      policy,
      hasSteamGridDbApiKey,
    );
  }

  async function refreshCardsAfterCoverSync(): Promise<string | null> {
    try {
      await refreshGameCards();
      return null;
    } catch (error) {
      return `${describeCommandError(error)} (covers may have downloaded; try Refresh Libraries.)`;
    }
  }

  function handleBackgroundCoverSyncError(error: unknown): void {
    setErrorMessage(`Background cover sync failed. ${describeCommandError(error)}`);
  }

  function setCoverAutoFetching(gameId: string, isFetching: boolean): void {
    if (isFetching) {
      coversAutoFetchingIds.add(gameId);
    } else {
      coversAutoFetchingIds.delete(gameId);
    }
  }

  function clearCoverAutoFetching(): void {
    coversAutoFetchingIds.clear();
  }

  // ---------------------------------------------------------------------------
  // Selection state
  // ---------------------------------------------------------------------------

  function clearSelectionIfSelectedGameMissing(): void {
    if (selectedGameId === null || hasGameCard(selectedGameId)) {
      return;
    }

    clearSelection();
  }

  function clearSelection(): void {
    detailsRequests.invalidate();

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

  function hasSelectedGameDetails(): boolean {
    return selectedDetails !== null;
  }

  function hasGameCard(gameId: string): boolean {
    return findGameCardForSelection(gameId, games) !== null;
  }

  function isSelectedGame(gameId: string): boolean {
    return selectedGameId !== null && areSameGameIds(selectedGameId, gameId);
  }

  function isPlanForGame(plan: SwapPlan | null, gameId: string): boolean {
    return plan !== null && areSameGameIds(plan.game_id, gameId);
  }

  function areSameGameIds(left: string, right: string): boolean {
    return normalizeSelectableGameId(left) === normalizeSelectableGameId(right);
  }

  // ---------------------------------------------------------------------------
  // Task / error helpers
  // ---------------------------------------------------------------------------

  async function runExclusive<T>(
    task: () => Promise<T>,
    options: ExclusiveTaskOptions = {},
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

  function showPartialScanWarning(errors: ScanError[]): void {
    setErrorMessage(formatPartialScanWarning(errors.length));
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
  selectedGameTitle={selectedShellGameTitle}
  {errorMessage}
  onNavigate={handleNavigate}
  onBack={handleBack}
>
  {#if screen === 'details'}
    <GameDetailsScreen
      details={selectedDetails}
      gameCard={currentGameCard}
      plan={currentPlan}
      {busy}
      onBuildPlan={handleBuildPlan}
      onApply={handleApply}
      onRollback={handleRollback}
    />
  {:else if screen === 'operations'}
    <OperationsScreen
      details={selectedDetails}
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
      {catalogVersion}
      {busy}
      {coversAutoFetchingIds}
      onScan={handleScan}
      onRefresh={scanAutoLibrariesAndRefreshCards}
      onReloadCards={handleReloadCards}
      onClearError={clearError}
      onCoverError={setErrorMessage}
      onOpenDetails={openGameDetails}
      onOpenOperations={openGameOperations}
    />
  {/if}
</DesktopShell>
