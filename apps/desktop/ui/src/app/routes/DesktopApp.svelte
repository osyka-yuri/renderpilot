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
  import { createCoverSyncQueue } from '@features/sync-covers';
  import {
    GameDetailsPage as GameDetailsScreen,
    createGameDetailsPageModel,
  } from '@pages/game-details';
  import { GamesPage as GamesScreen } from '@pages/games';
  import { OperationsPage as OperationsScreen } from '@pages/operations';
  import { SettingsPage as SettingsScreen, settingsTabMemory } from '@pages/settings';
  import { LibrariesPage as LibrariesScreen } from '@pages/libraries';
  import { createDesktopAppModel } from '@app/model/create-desktop-app-model.svelte';
  import {
    loadAndPresentGameDetails,
    openDesktopGame,
    refreshDesktopCatalog,
    reloadSelectedGame as reloadSelectedGameWorkflow,
    runUserCatalogRefresh,
    scanAutoLibrariesAndRefreshCards,
    scanManualFolderAndRefreshCards,
    syncMissingCoversAfterCardsLoad,
  } from '@app/model/desktop-app-workflows';
  import type { AppInitializationState } from '@entities/app';
  import {
    AppUpdateDialog,
    createAppUpdaterModel,
    createTauriAppUpdaterGateway,
  } from '@features/app-updater';

  type Props = {
    initState: AppInitializationState;
  };

  const { initState }: Props = $props();

  const model = createDesktopAppModel(() => initState);
  const coverSyncQueue = createCoverSyncQueue();
  const appUpdater = createAppUpdaterModel({
    gateway: createTauriAppUpdaterGateway(),
  });

  let refreshCounter = $state(0);
  let isRefreshing = $state(false);
  const gameDetailsModel = createGameDetailsPageModel({
    getSelectedGameId: () => model.selectedGameId,
    checkIsGameStillSelected: (gameId) => isGameSelected(model.selectedGameId, gameId),
    runExclusive: (task) => model.runExclusive(task),
    reloadGameDetails: () => reloadSelectedGame('details'),
  });

  /** Shared exclusive-lock + cover-sync deps for catalog mutations. */
  function catalogRefreshDeps() {
    return {
      runExclusive: <T,>(task: () => Promise<T>) => model.runExclusive(task),
      refreshGameCards,
      coverSyncQueue,
      syncMissingCoversAfterCardsLoad: () =>
        syncMissingCoversAfterCardsLoad({
          games: model.games,
          readSetting: getCatalogSetting,
          fetchGameCover,
          refreshGameCards,
          coverSyncQueue,
          beforeSync: () => tick(),
          onCoverReady: () => {
            // Re-run the games-page query as each cover lands so the grid
            // updates incrementally instead of only after the full batch.
            model.catalog.incrementCatalogVersion();
          },
        }),
    };
  }

  onMount(() => {
    model.applyCurrentTheme();

    const stopThemeObserver = observeSystemTheme(() => {
      model.applyCurrentTheme();
    });

    void appUpdater.start();
    void scanAutoLibrariesAndRefreshCards(catalogRefreshDeps());

    return () => {
      stopThemeObserver();
      void appUpdater.dispose();
    };
  });

  async function handleScan(): Promise<void> {
    await scanManualFolderAndRefreshCards(catalogRefreshDeps());
  }

  async function handleReloadCards(): Promise<void> {
    await model.runExclusive(refreshGameCards);
  }

  async function refreshGameCards(): Promise<void> {
    await refreshDesktopCatalog({
      setGames: model.catalog.setGames,
      incrementCatalogVersion: model.catalog.incrementCatalogVersion,
      clearSelectionIfSelectedGameMissing: model.clearSelectionIfSelectedGameMissing,
    });
  }

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

  function openRenoDxSettings(): void {
    settingsTabMemory.rememberTab('renodx');
    model.handleNavigate('settings');
  }

  async function handleRefresh(): Promise<void> {
    isRefreshing = true;
    try {
      await runUserCatalogRefresh(catalogRefreshDeps());
      refreshCounter++;
    } finally {
      isRefreshing = false;
    }
  }
</script>

<svelte:head>
  <title>RenderPilot Desktop</title>
</svelte:head>

<NotificationsToaster />

<AppUpdateDialog
  state={appUpdater.dialog}
  onInstall={() => void appUpdater.installAvailableUpdate()}
  onRetry={() => void appUpdater.retry()}
  onDismiss={() => void appUpdater.dismissDialog()}
  onRestart={() => void appUpdater.restartApplication()}
/>

<DesktopShell
  screen={model.screen}
  busy={model.busy}
  refreshing={isRefreshing}
  selectedGameTitle={model.selectedShellGameTitle}
  onNavigate={model.handleNavigate}
  onRefresh={handleRefresh}
  updateAvailable={appUpdater.notice !== null}
  updateOpening={appUpdater.settingsAction === 'checking'}
  onOpenUpdate={() => void appUpdater.openAvailableUpdate()}
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
        onOpenRenoDxSettings={openRenoDxSettings}
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
        appVersion={appUpdater.appVersion}
        updateAction={appUpdater.settingsAction}
        onThemeModeChange={model.changeThemeMode}
        onLanguageModeChange={model.changeLanguageMode}
        onCheckForUpdates={() => {
          if (appUpdater.settingsAction === 'open-update') {
            void appUpdater.openAvailableUpdate();
          } else {
            void appUpdater.checkForUpdates();
          }
        }}
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
