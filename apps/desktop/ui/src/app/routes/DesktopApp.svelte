<script lang="ts">
  import { onMount, tick } from 'svelte';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import type { WorkspaceScreen } from '@app/navigation/workspace';
  import { isGameSelected } from '@app/navigation/selection';
  import { fetchGameCover, getGameDetails } from '@entities/game';
  import { getCatalogSetting } from '@entities/settings';
  import { observeSystemTheme } from '@shared/theme';
  import { isDesktopPreviewMode } from '@shared/api-preview';
  import { ErrorBoundary } from '@shared/ui';
  import { NotificationsToaster } from '@widgets/notifications-toaster';
  import { ElevationBanner } from '@widgets/elevation-banner';
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
  import { LibrariesPage as LibrariesScreen } from '@pages/libraries';
  import { createDesktopAppModel } from '@app/model/create-desktop-app-model.svelte';
  import {
    loadAndPresentGameDetails,
    openDesktopGame,
    refreshDesktopCatalog,
    reloadSelectedGame as reloadSelectedGameWorkflow,
  } from '@app/model/desktop-app-workflows';
  import type { AppInitializationState } from '@entities/app';

  type Props = {
    initState: AppInitializationState;
  };

  const { initState }: Props = $props();

  const model = createDesktopAppModel(() => initState);
  const coverSyncQueue = createCoverSyncQueue();

  let refreshCounter = $state(0);
  let isRefreshing = $state(false);
  const gameDetailsModel = createGameDetailsPageModel({
    getSelectedGameId: () => model.selectedGameId,
    checkIsGameStillSelected: (gameId) => isGameSelected(model.selectedGameId, gameId),
    runExclusive: (task) => model.runExclusive(task),
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
    await refreshDesktopCatalog({
      setGames: model.catalog.setGames,
      incrementCatalogVersion: model.catalog.incrementCatalogVersion,
      clearSelectionIfSelectedGameMissing: model.clearSelectionIfSelectedGameMissing,
    });
  }

  // ---------------------------------------------------------------------------
  // Game details / operations
  // ---------------------------------------------------------------------------

  async function openGameDetails(gameId: string): Promise<void> {
    await openDesktopGame(gameId, 'details', {
      runExclusive: (task) => model.runExclusive(task),
      loadGameDetails,
    });
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    await loadAndPresentGameDetails(gameId, nextScreen, {
      getGameDetails,
      beginDetailsRequest: model.workspace.beginDetailsRequest,
      isDetailsRequestActive: model.workspace.isDetailsRequestActive,
      presentGameDetails: model.presentGameDetails,
    });
  }

  async function reloadSelectedGame(nextScreen: WorkspaceScreen): Promise<void> {
    await reloadSelectedGameWorkflow(nextScreen, {
      selectedGameId: model.selectedGameId,
      loadGameDetails,
    });
  }

  // ---------------------------------------------------------------------------
  // Background cover sync
  // ---------------------------------------------------------------------------

  async function handleRefresh(): Promise<void> {
    isRefreshing = true;
    try {
      await scanAutoLibrariesAndRefreshCards();
      refreshCounter++;
    } finally {
      isRefreshing = false;
    }
  }

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
      onCoverReady: () => {
        // Re-run the games-page query so each cover shows as soon as it downloads,
        // instead of all at once after the batch. The scheduler drops superseded
        // in-flight queries, so rapid completions coalesce to the latest result.
        model.catalog.incrementCatalogVersion();
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
  refreshing={isRefreshing}
  selectedGameTitle={model.selectedShellGameTitle}
  onNavigate={model.handleNavigate}
  onRefresh={handleRefresh}
>
  {#snippet banner()}
    <ElevationBanner isElevated={model.isElevated} elevationSupported={model.elevationSupported} />
  {/snippet}
  <ErrorBoundary>
    {#if model.screen === 'details'}
      <GameDetailsScreen
        details={model.selectedDetails}
        busy={model.busy}
        isElevated={model.isElevated}
        onSwap={gameDetailsModel.handleSwap}
        onRollback={gameDetailsModel.handleRollback}
        onBulkSwap={gameDetailsModel.handleBulkSwap}
        onBulkRollback={gameDetailsModel.handleBulkRollback}
        onOpenOperations={() => {
          model.handleNavigate('operations');
        }}
      />
    {:else if model.screen === 'operations'}
      <OperationsScreen details={model.selectedDetails} gameCard={model.currentGameCard} />
    {:else if model.screen === 'settings'}
      <SettingsScreen
        isElevated={model.isElevated}
        themeMode={model.themeMode}
        languageMode={model.languageMode}
        onThemeModeChange={model.changeThemeMode}
        onLanguageModeChange={model.changeLanguageMode}
      />
    {:else if model.screen === 'libraries'}
      <LibrariesScreen refreshKey={refreshCounter} />
    {:else}
      <GamesScreen
        games={model.games}
        catalogVersion={model.catalogVersion}
        busy={model.busy}
        coversAutoFetchingIds={coverSyncQueue.autoFetchingIds}
        pickCoverDisabled={isDesktopPreviewMode()}
        onScan={handleScan}
        onReloadCards={handleReloadCards}
        onClearError={model.clearError}
        onOpenDetails={openGameDetails}
      />
    {/if}
  </ErrorBoundary>
</DesktopShell>
