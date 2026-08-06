<script lang="ts">
  import { onMount, tick } from 'svelte';

  import DesktopShell from '@app/layout/DesktopShell.svelte';
  import type { Screen } from '@app/navigation/screen';
  import type { WorkspaceScreen } from '@app/navigation/workspace';
  import {
    isGameSelected,
    resolveSelectedGameCatalogDeltaAction,
    resolveSelectedWorkspaceTarget,
    resolveSelectedWorkspaceTargetForGame,
    type SelectedWorkspaceTarget,
    workspaceShellGameTitle,
  } from '@app/navigation/selection';
  import {
    bootstrapGamesCatalog,
    type CatalogDelta,
    fetchGameCover,
    findGameSummaryForSelection,
    getGameDetails,
  } from '@entities/game';
  import { rollbackComponent } from '@entities/operation';
  import { getCatalogSetting } from '@entities/settings';
  import { observeSystemTheme } from '@shared/theme';
  import { isDesktopPreviewMode } from '@shared/api-preview';
  import { ErrorBoundary } from '@shared/ui';
  import { NotificationsToaster } from '@widgets/notifications-toaster';
  import { ElevationBanner } from '@widgets/elevation-banner';
  import { createCoverSyncQueue } from '@features/sync-covers';
  import { createGameDetailsPageModel } from '@pages/game-details';
  import { GamesPage as GamesScreen, createGamesPageModel } from '@pages/games';
  import { settingsTabMemory } from '@pages/settings';
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
    refreshCatalogAndSelectedDetails,
    reloadSelectedGame as reloadSelectedGameWorkflow,
    runUserCatalogRefresh,
    rollbackRootCorrectionComponents,
    submitAddGameAndRefreshCards,
    removeGameAndRefreshCards,
    syncMissingCoversAfterCardsLoad,
  } from '@app/model/desktop-app-workflows';
  import { createSelectedGameDetailsRefresher } from '@app/model/create-selected-game-details-refresher';
  import { startBackgroundRefresh, type AppInitializationState } from '@entities/app';
  import { listen } from '@tauri-apps/api/event';
  import {
    AppUpdateDialog,
    createAppUpdaterModel,
    createTauriAppUpdaterGateway,
  } from '@features/app-updater';
  import {
    AddGameDialog,
    createAddGameFlow,
    inspectGameInstall,
    publishPartialLibraryScanWarning,
    selectGameInstallFolder,
  } from '@features/scan-libraries';
  import { presentError } from '@shared/error-presentation';
  import { ClientError, getErrorCode, reportClientError } from '@shared/errors';
  import { publishCommandErrorNotification } from '@shared/notifications';

  import LazyPage from './LazyPage.svelte';
  import { createDesktopPageRegistry } from './desktop-page-registry.svelte';

  type Props = {
    initState: AppInitializationState;
  };

  const { initState }: Props = $props();

  const model = createDesktopAppModel(() => initState);
  const selectedDetailsRefresher = createSelectedGameDetailsRefresher({
    getGameDetails,
    resolveCurrentTarget: (gameId) =>
      resolveSelectedWorkspaceTargetForGame(model.screen, model.selectedGameId, gameId),
    presentGameDetails: (details, target) => {
      model.presentGameDetails(details, target.screen);
    },
  });
  const pages = createDesktopPageRegistry();
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
      runExclusive: <T>(task: () => Promise<T>) => model.runExclusive(task),
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

  const addGameFlow = createAddGameFlow({
    chooseFolder: selectGameInstallFolder,
    inspect: inspectGameInstall,
    submit: async (inspection, confirmation) => {
      let commandError: unknown = null;
      const result = await submitAddGameAndRefreshCards(inspection, confirmation, {
        ...catalogRefreshDeps(),
        runExclusive: (task) =>
          model.runExclusive(task, {
            onError: (error) => {
              commandError = error;
            },
          }),
      });
      if (result !== null) {
        return { kind: 'completed' as const, result };
      }
      return commandError === null
        ? { kind: 'busy' as const }
        : { kind: 'failed' as const, error: commandError };
    },
    rollback: async (gameId, componentIds) => {
      let rollbackError: unknown = null;
      const result = await model.runExclusive(
        async () => {
          await rollbackRootCorrectionComponents(gameId, componentIds, {
            rollbackComponent,
            refreshGameCards,
          });
          return true as const;
        },
        {
          onError: (error) => {
            rollbackError = error;
          },
        },
      );
      if (result === true) {
        return { kind: 'completed' as const };
      }

      const error = rollbackError ?? new ClientError('add_game_rollback_failed');
      if (rollbackError === null) {
        reportClientError('rollback_root_correction', error);
      }
      return { kind: 'failed' as const, error };
    },
    presentError,
    presentCatalogBusyError: () => presentError(new ClientError('add_game_catalog_busy')),
    publishError: publishCommandErrorNotification,
    requiresReinspection: (error) => {
      const code = getErrorCode(error);
      return (
        code === 'root_correction_cleanup_required' ||
        code === 'root_correction_blocked' ||
        code === 'stale_install_inspection'
      );
    },
  });

  function preloadPage(screen: Screen): void {
    void pages.preload(screen);
  }

  function navigate(screen: Screen): void {
    preloadPage(screen);
    model.handleNavigate(screen);
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

  async function refreshSelectedGameDetails(
    target: SelectedWorkspaceTarget,
    operation: 'game_details_after_catalog_delta' | 'game_details_after_user_refresh',
  ): Promise<void> {
    try {
      await selectedDetailsRefresher.refresh(target);
    } catch (error: unknown) {
      reportClientError(operation, error);
    }
  }

  function handleCatalogDelta(delta: CatalogDelta): void {
    if (!gamesSession.acceptCatalogDelta(delta)) {
      return;
    }

    void gamesSession.refreshCatalog().catch((error: unknown) => {
      reportClientError('catalog_read_after_delta', error);
    });

    const action = resolveSelectedGameCatalogDeltaAction(model.screen, model.selectedGameId, delta);
    switch (action.kind) {
      case 'none':
        return;
      case 'clear':
        selectedDetailsRefresher.cancel();
        model.clearSelection();
        return;
      case 'reload':
        void refreshSelectedGameDetails(action, 'game_details_after_catalog_delta');
    }
  }

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
        reportClientError('catalog_read_after_background_refresh', error);
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
      onCatalogDelta: handleCatalogDelta,
      onPartialScanFailures: publishPartialLibraryScanWarning,
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
        reportClientError('bootstrap_games_catalog', error);
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
      selectedDetailsRefresher.dispose();
      catalogLifecycle.dispose();
      gamesSession.flushSearchPersist();
      gamesSession.dispose();
      void appUpdater.dispose();
    };
  });

  async function handleAddGame(): Promise<void> {
    await addGameFlow.chooseFolder();
  }

  async function handleRemoveGame(gameId: string): Promise<boolean> {
    let removalError: unknown = null;
    const removed = await removeGameAndRefreshCards(gameId, {
      runExclusive: (task) =>
        model.runExclusive(task, {
          onError: (error) => {
            removalError = error;
          },
        }),
      refreshGameCards,
    });
    if (removalError !== null) {
      throw removalError instanceof Error
        ? removalError
        : new ClientError('unexpected_client_error', removalError);
    }
    return removed;
  }

  async function refreshGameCards(): Promise<void> {
    await gamesSession.refreshCatalog();
  }

  async function openGameDetails(gameId: string): Promise<void> {
    await openDesktopGame(gameId, 'details', {
      preloadPage: () => {
        void pages.details.preload();
      },
      runExclusive: (task) => model.runExclusive(task),
      loadGameDetails,
    });
  }

  async function loadGameDetails(gameId: string, nextScreen: WorkspaceScreen): Promise<void> {
    // Foreground navigation always wins over passive catalog-driven refreshes.
    selectedDetailsRefresher.cancel();
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
    navigate('settings');
  }

  function handleRefresh(): void {
    if (isRefreshing) {
      return;
    }

    isRefreshing = true;
    void refreshCatalogAndSelectedDetails({
      refreshCatalog: () => runUserCatalogRefresh(catalogRefreshDeps()),
      markCatalogRefreshed: () => {
        refreshCounter++;
      },
      resolveSelectedTarget: () =>
        resolveSelectedWorkspaceTarget(model.screen, model.selectedGameId),
      refreshSelectedDetails: (target) =>
        refreshSelectedGameDetails(target, 'game_details_after_user_refresh'),
    })
      .catch((error: unknown) => {
        reportClientError('user_catalog_refresh', error);
      })
      .finally(() => {
        isRefreshing = false;
      });
  }
</script>

<svelte:head>
  <title>RenderPilot Desktop</title>
</svelte:head>

<NotificationsToaster />

{#if addGameFlow.dialog !== null}
  <AddGameDialog
    state={addGameFlow.dialog}
    onClose={addGameFlow.close}
    onChooseFolder={addGameFlow.chooseFolder}
    onConfirm={addGameFlow.confirm}
    onRollbackAndConfirm={addGameFlow.rollbackAndConfirm}
  />
{/if}

{#if appUpdater.dialog !== null}
  <AppUpdateDialog
    state={appUpdater.dialog}
    onInstall={() => void appUpdater.installAvailableUpdate()}
    onRetry={() => void appUpdater.retry()}
    onDismiss={() => void appUpdater.dismissDialog()}
    onRestart={() => void appUpdater.restartApplication()}
  />
{/if}

<DesktopShell
  screen={model.screen}
  busy={model.busy}
  refreshing={isRefreshing}
  selectedGameTitle={selectedShellGameTitle}
  onNavigate={navigate}
  onPreload={preloadPage}
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
      <LazyPage
        page={pages.details}
        onBack={() => {
          navigate('games');
        }}
      >
        {#snippet children(GameDetailsScreen)}
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
            onPreloadRenoDxSettings={() => {
              preloadPage('settings');
            }}
            onOpenOperations={() => {
              navigate('operations');
            }}
            onPreloadOperations={() => {
              preloadPage('operations');
            }}
          />
        {/snippet}
      </LazyPage>
    {:else if model.screen === 'operations'}
      <LazyPage
        page={pages.operations}
        onBack={() => {
          navigate('games');
        }}
      >
        {#snippet children(OperationsScreen)}
          <OperationsScreen details={model.selectedDetails} gameCard={currentGameCard} />
        {/snippet}
      </LazyPage>
    {:else if model.screen === 'settings'}
      <LazyPage
        page={pages.settings}
        onBack={() => {
          navigate('games');
        }}
      >
        {#snippet children(SettingsScreen)}
          <SettingsScreen
            isElevated={model.isElevated}
            themeMode={model.themeMode}
            languageMode={model.languageMode}
            languageBusy={model.languageBusy}
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
        {/snippet}
      </LazyPage>
    {:else if model.screen === 'libraries'}
      <LazyPage
        page={pages.libraries}
        onBack={() => {
          navigate('games');
        }}
      >
        {#snippet children(LibrariesScreen)}
          <LibrariesScreen refreshKey={refreshCounter} />
        {/snippet}
      </LazyPage>
    {:else}
      <GamesScreen
        session={gamesSession}
        busy={model.busy || addGameFlow.busy}
        coversAutoFetchingIds={coverSyncQueue.autoFetchingIds}
        pickCoverDisabled={isDesktopPreviewMode()}
        onAddGame={handleAddGame}
        onRemoveGame={handleRemoveGame}
        onOpenDetails={openGameDetails}
        onPreloadDetails={() => {
          preloadPage('details');
        }}
      />
    {/if}
  </ErrorBoundary>
</DesktopShell>
