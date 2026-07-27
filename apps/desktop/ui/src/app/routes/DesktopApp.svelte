<script lang="ts">
  import { onMount, tick } from 'svelte';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import type { WorkspaceScreen } from '@app/navigation/workspace';
  import { isGameSelected, workspaceShellGameTitle } from '@app/navigation/selection';
  import {
    bootstrapGamesCatalog,
    type CatalogDelta,
    fetchGameCover,
    findGameSummaryForSelection,
    getGameDetails,
  } from '@entities/game';
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
  import { GamesPage as GamesScreen, createGamesPageModel } from '@pages/games';
  import { LibrariesPage as LibrariesScreen } from '@pages/libraries';
  import { OperationsPage as OperationsScreen } from '@pages/operations';
  import { SettingsPage as SettingsScreen, settingsTabMemory } from '@pages/settings';
  import { createDesktopAppModel } from '@app/model/create-desktop-app-model.svelte';
  import {
    createInitialCatalogLifecycle,
    type CatalogEventPayloads,
    type InitialCatalogSyncCompletion,
  } from '@app/model/initial-catalog-lifecycle';
  import {
    loadAndPresentGameDetails,
    openDesktopGame,
    queueBackgroundCoverSync,
    reloadSelectedGame as reloadSelectedGameWorkflow,
    runUserCatalogRefresh,
    scanManualFolderAndRefreshCards,
    syncMissingCoversAfterCardsLoad,
  } from '@app/model/desktop-app-workflows';
  import { startBackgroundRefresh, type AppInitializationState } from '@entities/app';
  import { listen } from '@tauri-apps/api/event';
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
  const gamesSession = createGamesPageModel({
    getCoversAutoFetchingIds: () => coverSyncQueue.autoFetchingIds,
    getOnClearError: () => model.clearError,
  });
  const currentGameCard = $derived(
    findGameSummaryForSelection(model.selectedGameId, gamesSession.games),
  );
  const selectedShellGameTitle = $derived(
    workspaceShellGameTitle(currentGameCard, model.selectedDetails),
  );

  let refreshCounter = $state(0);
  let isRefreshing = $state(false);
  let backgroundCoverHydrationEnabled = $state(false);
  let lastCoverHydrationScope = '';
  const gameDetailsModel = createGameDetailsPageModel({
    getSelectedGameId: () => model.selectedGameId,
    checkIsGameStillSelected: (gameId) => isGameSelected(model.selectedGameId, gameId),
    runExclusive: (task) => model.runExclusive(task),
    reloadGameDetails: reloadDetailsAndCatalog,
  });

  /** Shared exclusive-lock + cover-sync deps for catalog mutations. */
  function catalogRefreshDeps() {
    return {
      runExclusive: <T,>(task: () => Promise<T>) => model.runExclusive(task),
      refreshGameCards,
      coverSyncQueue,
      syncMissingCoversAfterCardsLoad: () =>
        syncMissingCoversAfterCardsLoad({
          games: gamesSession.games,
          readSetting: getCatalogSetting,
          fetchGameCover,
          coverSyncQueue,
          beforeSync: () => tick(),
          onCoverReady: (gameId, result) => {
            gamesSession.patchCover(gameId, result.updated_at_ms);
          },
        }),
    };
  }

  function queueMissingCoverHydration(): void {
    queueBackgroundCoverSync(catalogRefreshDeps());
  }

  $effect(() => {
    if (!backgroundCoverHydrationEnabled || gamesSession.bootstrapping) {
      return;
    }
    const scope = gamesSession.games
      .map((game) => game.game_id)
      .sort()
      .join('\u0000');
    if (scope === lastCoverHydrationScope) {
      return;
    }
    lastCoverHydrationScope = scope;
    queueMissingCoverHydration();
  });

  onMount(() => {
    model.applyCurrentTheme();

    const stopThemeObserver = observeSystemTheme(() => {
      model.applyCurrentTheme();
    });

    let disposed = false;
    const isDisposed = () => disposed;
    const completeInitialCatalogSync = async ({
      forceCatalogRefresh,
    }: InitialCatalogSyncCompletion) => {
      if (disposed) {
        return;
      }
      try {
        if (forceCatalogRefresh || gamesSession.catalogSize === 0) {
          await gamesSession.refreshCatalog();
        }
      } catch (error: unknown) {
        console.error('Failed to read catalog after background refresh.', error);
      } finally {
        if (!isDisposed()) {
          gamesSession.completeInitialCatalogSync();
        }
      }
    };
    const catalogLifecycle = createInitialCatalogLifecycle({
      previewMode: isDesktopPreviewMode(),
      listenEvent: (event, onPayload) =>
        listen<CatalogEventPayloads[typeof event]>(event, ({ payload }) => {
          onPayload(payload);
        }),
      startBackgroundRefresh,
      startUpdater: () => {
        void appUpdater.start();
      },
      onCatalogDelta: (delta: CatalogDelta) => {
        if (gamesSession.acceptCatalogDelta(delta)) {
          void gamesSession.refreshCatalog();
        }
      },
      completeInitialCatalogSync,
      enableCoverHydration: () => {
        backgroundCoverHydrationEnabled = true;
      },
    });
    void bootstrapGamesCatalog().then(
      async (bootstrap) => {
        if (isDisposed()) {
          return;
        }
        gamesSession.applyBootstrap(bootstrap);
        if (gamesSession.pendingCatalogDelta !== null) {
          await gamesSession.refreshCatalog();
        }
        await tick();
        catalogLifecycle.startServices();
      },
      async (error: unknown) => {
        console.error('Failed to bootstrap games catalog.', error);
        try {
          await gamesSession.loadFilterPreferences(isDisposed);
          if (!isDisposed()) {
            await model.runExclusive(refreshGameCards);
          }
        } finally {
          if (!isDisposed()) {
            gamesSession.completeBootstrapRecovery();
            await tick();
            catalogLifecycle.startServices();
          }
        }
      },
    );

    return () => {
      stopThemeObserver();
      disposed = true;
      catalogLifecycle.dispose();
      gamesSession.flushSearchPersist();
      gamesSession.dispose();
      void appUpdater.dispose();
    };
  });

  async function handleScan(): Promise<void> {
    await scanManualFolderAndRefreshCards(catalogRefreshDeps());
  }

  async function refreshGameCards(): Promise<void> {
    await gamesSession.refreshCatalog();
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

  async function reloadDetailsAndCatalog(): Promise<void> {
    await reloadSelectedGame('details');
    await gamesSession.refreshCatalog();
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
  selectedGameTitle={selectedShellGameTitle}
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
        onGameDetailsInvalidate={reloadDetailsAndCatalog}
        onOpenRenoDxSettings={openRenoDxSettings}
        onOpenOperations={() => {
          model.handleNavigate('operations');
        }}
      />
    {:else if model.screen === 'operations'}
      <OperationsScreen details={model.selectedDetails} gameCard={currentGameCard} />
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
        session={gamesSession}
        busy={model.busy}
        coversAutoFetchingIds={coverSyncQueue.autoFetchingIds}
        pickCoverDisabled={isDesktopPreviewMode()}
        onScan={handleScan}
        onOpenDetails={openGameDetails}
      />
    {/if}
  </ErrorBoundary>
</DesktopShell>
