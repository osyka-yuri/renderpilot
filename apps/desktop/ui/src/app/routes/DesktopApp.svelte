<script lang="ts">
  import { onMount, tick } from 'svelte';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import type { WorkspaceScreen } from '@app/navigation/workspace';
  import { getScreenAfterRollback } from '@app/navigation/workspace';
  import { isGameSelected } from '@app/navigation/selection';
  import {
    fetchGameCover,
    getGameDetails,
    queryGameCards,
    DEFAULT_GAME_CARDS_CATALOG_PAGE,
    DEFAULT_GAME_CARDS_CATALOG_SORT,
    normalizeSelectableGameId,
  } from '@entities/game';
  import { publishRollbackCompletedNotification, rollbackOperation } from '@entities/operation';
  import { getCatalogSetting } from '@entities/settings';
  import { observeSystemTheme } from '@shared/theme';
  import { isDesktopPreviewMode } from '@shared/api-preview';
  import { NotificationsToaster } from '@widgets/notifications-toaster';
  import {
    createCoverSyncQueue,
    executeBackgroundCoverSync,
    publishBackgroundCoverSyncFailureNotification,
    publishBackgroundCoverSyncIssueNotification,
  } from '@features/sync-covers';
  import {
    publishAutomaticLibraryScanFailedNotification,
    publishPartialLibraryScanWarning,
    scanAutoLibrariesWithErrorRecovery,
    selectManualScanFolder,
    scanManualFolder,
  } from '@features/scan-libraries';
  import {
    GameDetailsPage as GameDetailsScreen,
    createGameDetailsPageModel,
  } from '@pages/game-details';
  import { GamesPage as GamesScreen } from '@pages/games';
  import { OperationsPage as OperationsScreen } from '@pages/operations';
  import { SettingsPage as SettingsScreen } from '@pages/settings';
  import { createDesktopAppModel } from '@app/model/create-desktop-app-model.svelte';

  const model = createDesktopAppModel();
  const coverSyncQueue = createCoverSyncQueue();
  const gameDetailsModel = createGameDetailsPageModel({
    getSelectedGameId: () => model.selectedGameId,
    checkIsGameStillSelected: (gameId) => isGameSelected(model.selectedGameId, gameId),
    runExclusive: (task) => model.runExclusive(task),
    setCurrentPlan: (plan) => {
      model.setCurrentPlan(plan);
    },
    getCurrentPlan: (id) => model.getCurrentPlan(id),
    showStalePlanError: () => {
      model.showStalePlanError();
    },
    reloadGameDetails: () => reloadSelectedGame('details'),
  });

  onMount(() => {
    model.applyCurrentTheme();

    const stopThemeObserver = observeSystemTheme(() => {
      model.applyCurrentTheme();
    });

    void scanAutoLibrariesAndRefreshCards();

    return stopThemeObserver;
  });

  // ---------------------------------------------------------------------------
  // Catalog loading
  // ---------------------------------------------------------------------------

  async function scanAutoLibrariesAndRefreshCards(): Promise<void> {
    await runCatalogRefreshWithCoverSync(async () => {
      const scanResult = await scanAutoLibrariesWithErrorRecovery();

      if (scanResult.kind === 'error') {
        publishAutomaticLibraryScanFailedNotification(scanResult.message);
        return true;
      }

      if (scanResult.errors.length > 0) {
        publishPartialLibraryScanWarning(scanResult.errors.length);
      }

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
    await model.runExclusive(refreshGameCards);
  }

  async function runCatalogRefreshWithCoverSync(
    prepareRefresh: () => Promise<boolean>,
  ): Promise<void> {
    const refreshed = await model.runExclusive(async () => {
      const shouldRefresh = await prepareRefresh();

      if (!shouldRefresh) {
        return false;
      }

      await refreshGameCards();
      return true;
    });

    if (refreshed === true) {
      coverSyncQueue.queue(syncMissingCoversAfterCardsLoad, (error) => {
        publishBackgroundCoverSyncFailureNotification(error);
      });
    }
  }

  async function refreshGameCards(): Promise<void> {
    const result = await queryGameCards({
      searchQuery: '',
      selectedLibraries: [],
      sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
      page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
    });

    model.catalog.setGames(result.items);
    model.catalog.incrementCatalogVersion();

    model.clearSelectionIfSelectedGameMissing();
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

    await model.runExclusive(() => loadGameDetails(normalizedGameId, nextScreen));
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    const requestToken = model.workspace.beginDetailsRequest();
    const details = await getGameDetails(gameId);

    if (!model.workspace.isDetailsRequestActive(requestToken)) {
      return;
    }

    model.presentGameDetails(details, nextScreen);
  }

  async function reloadSelectedGame(nextScreen: WorkspaceScreen): Promise<void> {
    const gameId = model.selectedGameId;

    if (gameId === null) {
      return;
    }

    await loadGameDetails(gameId, nextScreen);
  }

  async function handleRollback(operationId: string): Promise<void> {
    const result = await model.runExclusive(async () => {
      const rollbackResult = await rollbackOperation(operationId);

      model.setCurrentPlan(null);

      await reloadSelectedGame(getScreenAfterRollback(model.screen));

      return rollbackResult;
    });

    if (result !== null) {
      publishRollbackCompletedNotification(result.items.length);
    }
  }

  // ---------------------------------------------------------------------------
  // Background cover sync
  // ---------------------------------------------------------------------------

  async function syncMissingCoversAfterCardsLoad(): Promise<void> {
    await tick();

    const cardSnapshot = model.games.slice();

    if (cardSnapshot.length === 0) {
      return;
    }

    await executeBackgroundCoverSync(cardSnapshot, {
      readSetting: getCatalogSetting,
      fetchGameCover,
      refreshGameCards,
      onGameStart: (gameId) => {
        coverSyncQueue.setAutoFetching(gameId, true);
      },
      onGameEnd: (gameId) => {
        coverSyncQueue.setAutoFetching(gameId, false);
      },
      onError: publishBackgroundCoverSyncIssueNotification,
    });
  }
</script>

<svelte:head>
  <title>RenderPilot Desktop</title>
</svelte:head>

<NotificationsToaster />

<DesktopShell
  screen={model.screen}
  busy={model.busy}
  selectedGameTitle={model.selectedShellGameTitle}
  onNavigate={model.handleNavigate}
>
  {#if model.screen === 'details'}
    <GameDetailsScreen
      details={model.selectedDetails}
      gameCard={model.currentGameCard}
      plan={model.currentPlan}
      busy={model.busy}
      onBuildPlan={gameDetailsModel.handleBuildPlan}
      onApply={gameDetailsModel.handleApply}
      onRollback={handleRollback}
    />
  {:else if model.screen === 'operations'}
    <OperationsScreen
      details={model.selectedDetails}
      gameCard={model.currentGameCard}
      busy={model.busy}
      onRollback={handleRollback}
      onViewGame={() => {
        model.handleNavigate('details');
      }}
    />
  {:else if model.screen === 'settings'}
    <SettingsScreen
      themeMode={model.themeMode}
      languageMode={model.languageMode}
      advancedMode={model.advancedMode}
      onThemeModeChange={model.changeThemeMode}
      onLanguageModeChange={model.changeLanguageMode}
      onToggleAdvancedMode={model.toggleAdvancedMode}
    />
  {:else}
    <GamesScreen
      games={model.games}
      catalogVersion={model.catalogVersion}
      busy={model.busy}
      coversAutoFetchingIds={coverSyncQueue.autoFetchingIds}
      pickCoverDisabled={isDesktopPreviewMode()}
      onScan={handleScan}
      onRefresh={scanAutoLibrariesAndRefreshCards}
      onReloadCards={handleReloadCards}
      onClearError={model.clearError}
      onOpenDetails={openGameDetails}
      onOpenOperations={openGameOperations}
    />
  {/if}
</DesktopShell>
