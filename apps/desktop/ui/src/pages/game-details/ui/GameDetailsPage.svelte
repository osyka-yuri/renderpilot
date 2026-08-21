<script lang="ts">
  import type { GameCandidateGroup, GameDetails, GameLibraryComponent } from '@entities/game';
  import {
    ADDONS_TAB_VALUE,
    createGameDetailsTabs,
    reconcileGameDetailsTabValue,
    NVIDIA_STREAMLINE_TECHNOLOGY,
    DLSS_FAMILY_CARDS,
  } from '../model/game-details-tabs';
  import {
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
    Card,
    CardContent,
    CardDescription,
    CardTitle,
    Progress,
    ScrollArea,
    Button,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import ArrowUpToLineIcon from '@lucide/svelte/icons/arrow-up-to-line';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { t } from '@shared/i18n';
  import { DesktopCommandError, isFileSafetyContextError, reportClientError } from '@shared/errors';
  import { sumDownloadFractions } from '@shared/lib';
  import { publishPresentedErrorNotification } from '@shared/notifications';
  import type { SettingFamily } from '@features/nvapi-settings';
  import { RenoDxCard } from '@features/renodx';
  import { LumaCard } from '@features/luma';
  import type {
    SwapHandler,
    RollbackHandler,
    BulkSwapHandler,
    BulkRollbackHandler,
  } from '../model/create-game-details-page-model';
  import { buildUpdateAllToLatestPlan } from '../model/update-all-to-latest';
  import { UpdateAllError } from '../model/run-update-all';
  import { createGameAddonsContext } from '../model/create-game-addons-context.svelte';
  import { createFileSafetyContext } from '../model/create-file-safety-context.svelte';
  import type { FileSafetyScope } from '../model/create-file-safety-context.svelte';
  import { createUpdateAllWorkflow } from '../model/create-update-all-workflow.svelte';
  import { createNvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import { createGameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type { MutationSafetyTokens } from '@entities/addon';
  import type { SwapRequest } from '../model/swap-request';
  import { resolveExecutableLockReason } from '../model/game-executable-lock';
  import GameExecutablePopover from './GameExecutablePopover.svelte';
  import NvidiaProfileCard from './NvidiaProfileCard.svelte';
  import DlssComponentCard from './DlssComponentCard.svelte';
  import StreamlineComponentCard from './StreamlineComponentCard.svelte';
  import D3d12ExecutableConfirmDialog from './D3d12ExecutableConfirmDialog.svelte';
  import DeveloperModeRequirementDialog from './DeveloperModeRequirementDialog.svelte';
  import VendorComponentCard from './VendorComponentCard.svelte';
  import { areSameGameIds, GameFileSafetyRow } from '@entities/game';
  import { onDestroy, untrack } from 'svelte';

  type Props = {
    details?: GameDetails | null;
    busy?: boolean;
    onSwap?: SwapHandler;
    onRollback?: RollbackHandler;
    onBulkSwap?: BulkSwapHandler;
    onBulkRollback?: BulkRollbackHandler;
    onOpenOperations?: () => void;
    onPreloadOperations?: () => void;
    onOpenRenoDxSettings?: () => void;
    onPreloadRenoDxSettings?: () => void;
    onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  };

  const {
    details = null,
    busy = false,
    onSwap = () => undefined,
    onRollback = () => undefined,
    onBulkSwap = () => undefined,
    onBulkRollback = () => undefined,
    onOpenOperations,
    onPreloadOperations = () => undefined,
    onOpenRenoDxSettings = () => undefined,
    onPreloadRenoDxSettings = () => undefined,
    onGameDetailsInvalidate = () => undefined,
  }: Props = $props();

  const tabs = $derived(createGameDetailsTabs(details));
  const vendorTabs = $derived(tabs.vendorTabs);
  const gameId = $derived(details?.game.identity.id ?? null);
  const executableLockReason = $derived(resolveExecutableLockReason(details?.components ?? []));
  // The game's launcher, for Luma's launcher-aware launch-args callout.
  const launcher = $derived(details?.game.identity.launcher ?? '');

  const fileSafety = createFileSafetyContext({ getGameId: () => gameId });
  // Update All deliberately keeps one captured assessment for every step. A
  // refreshed context is used by the next user action, while this run stops on
  // the stale token instead of silently switching authorization mid-batch.
  let updateAllSafetyTokens = $state<MutationSafetyTokens | null | undefined>(undefined);
  let capturingUpdateAllSafety = $state(false);

  async function requirePageSafetyTokens(
    requestedGameId: string,
    scope: FileSafetyScope,
  ): Promise<MutationSafetyTokens> {
    if (!gameId || !areSameGameIds(requestedGameId, gameId)) {
      throw DesktopCommandError.fromDto({ code: 'safety_context_scope_mismatch' });
    }
    if (updateAllSafetyTokens) {
      return updateAllSafetyTokens;
    }
    return fileSafety.requireTokens(scope);
  }

  const gameAddons = createGameAddonsContext({
    getGameId: () => gameId,
    getCapabilities: () => tabs.addonsTab?.capabilities ?? [],
    onGameDetailsInvalidate: (id) => onGameDetailsInvalidate(id),
    requireSafetyTokens: (id, scope) => requirePageSafetyTokens(id, scope),
    onSafetyContextError: (error, scope) => fileSafety.refreshForMutationError(error, scope),
  });
  const { renodx, luma } = gameAddons.stores;

  // The single "update everything to its latest version" action. Spans every
  // vendor (NVIDIA/AMD/Intel) plus the Streamline bundle, RenoDX, and Luma, not
  // just the active tab, and reuses the existing bulk-swap path.
  const updatePlan = $derived(buildUpdateAllToLatestPlan(details));
  const totalUpdateCount = $derived(updatePlan.updateCount + gameAddons.updateCount);
  const nothingToUpdate = $derived(totalUpdateCount === 0);

  const updateAllWorkflow = createUpdateAllWorkflow({
    getGameId: () => gameId,
    getPlan: () => updatePlan,
    getAddonUpdates: () => gameAddons.addonUpdates,
    hasUpdates: () => !nothingToUpdate,
    isBusy: () => busy || gameAddons.busy,
    onBulkSwap: (items) => handleBulkSwapWithSafety(items),
    onError: reportUpdateAllError,
  });
  const updatingAll = $derived(updateAllWorkflow.updating);
  const planningUpdateAll = $derived(updateAllWorkflow.planning);
  const updateConfirmOpen = $derived(updateAllWorkflow.confirmationOpen);
  const preparedUpdateExecutableActions = $derived(updateAllWorkflow.confirmationActions);
  const pendingDownloadIds = $derived(updateAllWorkflow.pendingDownloadIds);
  let updateAllOwnerGameId: string | null | undefined;

  $effect(() => {
    const currentGameId = gameId;
    if (updateAllOwnerGameId === undefined) {
      updateAllOwnerGameId = currentGameId;
      return;
    }
    if (currentGameId !== updateAllOwnerGameId) {
      updateAllOwnerGameId = currentGameId;
      untrack(() => {
        updateAllWorkflow.invalidatePending();
      });
    }
  });

  onDestroy(() => {
    updateAllWorkflow.destroy();
    gameAddons.destroy();
    fileSafety.destroy();
  });
  // Shared exclusive gate for Luma/RenoDX cards (peer mutations + Update-all).
  const exclusiveBusy = $derived(busy || gameAddons.busy || updatingAll || planningUpdateAll);
  const showProgress = $derived(updatingAll && pendingDownloadIds.length > 0);
  const downloadCount = $derived(pendingDownloadIds.length);
  const downloadValue = $derived(showProgress ? sumDownloadFractions(pendingDownloadIds) : 0);

  function updateAllSafetyScope(): FileSafetyScope {
    const includesRenoDx = gameAddons.addonUpdates.some(({ step }) => step === 'renodx');
    if (!includesRenoDx) {
      return 'game';
    }
    return renodx.state?.status === 'installed' && renodx.state.host_kind === 'proxy'
      ? 'game'
      : 'game_and_shared';
  }

  async function handleUpdateAll(): Promise<void> {
    if (
      !gameId ||
      capturingUpdateAllSafety ||
      updatingAll ||
      planningUpdateAll ||
      gameAddons.busy ||
      busy ||
      nothingToUpdate
    ) {
      return;
    }
    capturingUpdateAllSafety = true;
    try {
      // Capture one context for the complete batch. Individual steps reuse it,
      // so a stale token stops the batch instead of switching authorization
      // halfway through Update All.
      updateAllSafetyTokens = await fileSafety.requireTokens(updateAllSafetyScope());
      await updateAllWorkflow.start();
    } catch (error) {
      reportUpdateAllError(error, true);
    } finally {
      capturingUpdateAllSafety = false;
    }
  }

  $effect(() => {
    const workflowActive =
      updatingAll ||
      planningUpdateAll ||
      updateConfirmOpen ||
      updateAllWorkflow.developerModeOpen ||
      updateAllWorkflow.developerModeRetrying;
    if (!workflowActive && updateAllSafetyTokens !== undefined) {
      untrack(() => {
        updateAllSafetyTokens = undefined;
      });
    }
  });

  async function handleSwapWithSafety(request: Parameters<SwapHandler>[0]): Promise<void> {
    try {
      if (!gameId) {
        throw DesktopCommandError.fromDto({ code: 'safety_context_missing' });
      }
      const tokens = await requirePageSafetyTokens(gameId, 'game');
      await onSwap({ ...request, gameContextToken: tokens.gameContextToken });
    } catch (error) {
      await fileSafety.refreshForMutationError(error, 'game');
      throw error;
    }
  }

  async function handleBulkSwapWithSafety(items: readonly SwapRequest[]): Promise<void> {
    try {
      if (!gameId) {
        throw DesktopCommandError.fromDto({ code: 'safety_context_missing' });
      }
      const tokens = await requirePageSafetyTokens(gameId, 'game');
      await onBulkSwap(
        items.map((item) => ({
          ...item,
          gameContextToken: tokens.gameContextToken,
        })),
      );
    } catch (error) {
      await fileSafety.refreshForMutationError(error, 'game');
      throw error;
    }
  }

  function reportUpdateAllError(error: unknown, notifySafety = false): void {
    const failureCount = error instanceof UpdateAllError ? error.failures.length : 1;
    const primaryError =
      error instanceof UpdateAllError ? (error.failures[0]?.error ?? error) : error;
    if (notifySafety || !isFileSafetyContextError(primaryError)) {
      publishPresentedErrorNotification(
        t('gameDetails.updateAll.partialFailure', { count: failureCount }),
        primaryError,
      );
    }
    reportClientError('update_all_workflow', primaryError);
  }

  const hasNvidiaTab = $derived(vendorTabs.some((tab) => tab.key === 'nvidia'));

  // The active tab is user-controlled state, not derived: a post-swap
  // details reload re-derives `tabs`, and a hardcoded `value={tabs[0].key}`
  // would snap the user back to the first tab every time. Reconcile
  // only when the set of available tabs changes — keep the current selection if
  // it is still available, otherwise fall back to the first tab.
  let selectedTab = $state('');
  $effect(() => {
    const available = tabs.values;
    untrack(() => {
      selectedTab = reconcileGameDetailsTabValue(selectedTab, available);
    });
  });

  /**
   * Fingerprint of all installed DLSS DLLs. Changes when the user swaps any of
   * them (the new file has a different sha256 / version), which we read inside
   * the NVAPI reload effect so the DLL info badge and the supported-value lists
   * stay in sync without requiring a page revisit.
   */
  const dlssFingerprint = $derived.by(() => {
    if (!details) {
      return null;
    }
    return details.components
      .filter((c) => c.technology in DLSS_FAMILY_CARDS)
      .map((c) => c.files[0]?.sha256 ?? c.files[0]?.version ?? '')
      .join('|');
  });

  // ── Single NVIDIA driver context, owned by the page ──────────────
  // Owns every DLSS setting's live state plus the profile executable
  // selection. One reload covers both, so changing the executable refreshes
  // every family card's values.
  const nvidia = createNvidiaDriverContext();

  // The executable is a game-level identity feeding both the NVIDIA profile target
  // and the RenoDX install location, so it lives above the tabs in its own context.
  // Changing it re-reads the NVIDIA settings (they key off the profile's exe).
  const gameExe = createGameExecutableContext({
    onChange: async (id) => {
      await Promise.all([
        hasNvidiaTab ? nvidia.reload(id) : Promise.resolve(),
        onGameDetailsInvalidate(id),
      ]);
    },
  });

  $effect(() => {
    const id = gameId;

    untrack(() => {
      if (!id) {
        gameExe.clear();
        return;
      }

      void gameExe.reload(id);
    });
  });

  $effect(() => {
    // Explicit reactive dependencies:
    //   - gameId / hasNvidiaTab: standard load/teardown
    //   - dlssFingerprint:       re-load after any DLSS DLL swap
    const id = gameId;
    const shouldLoad = hasNvidiaTab;
    void dlssFingerprint;

    untrack(() => {
      if (!id || !shouldLoad) {
        nvidia.clear();
        return;
      }

      void nvidia.reload(id);
    });
  });

  function getCandidateGroup(componentId: string): GameCandidateGroup | null {
    return details?.candidate_groups.find((g) => g.component_id === componentId) ?? null;
  }

  function dlssFamilyCard(
    component: GameLibraryComponent,
  ): { family: SettingFamily; title: string } | null {
    return DLSS_FAMILY_CARDS[component.technology] ?? null;
  }

  function isStreamline(component: GameLibraryComponent): boolean {
    return component.technology === NVIDIA_STREAMLINE_TECHNOLOGY;
  }
</script>

<section class="flex h-full min-h-0 flex-col overflow-hidden" aria-labelledby="game-details-title">
  <h1 id="game-details-title" class="sr-only">
    {details?.game.identity.title ?? t('nav.gameFallback')}
  </h1>
  {#if !details}
    <Card>
      <CardContent>
        <CardTitle level={2}>{t('gameDetails.noGameSelected.title')}</CardTitle>
        <CardDescription>
          {t('gameDetails.noGameSelected.description')}
        </CardDescription>
      </CardContent>
    </Card>
  {:else if gameId}
    <!-- Keep tab controls visible while the notice and active tab content share one viewport. -->
    <Tabs bind:value={selectedTab} class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
      <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
        {#if tabs.values.length > 0}
          <TabsList aria-label={details.game.identity.title}>
            {#each vendorTabs as tab (tab.key)}
              <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
            {/each}
            {#if tabs.addonsTab}
              <TabsTrigger value={tabs.addonsTab.value}>{t('gameDetails.otherTab')}</TabsTrigger>
            {/if}
          </TabsList>
        {/if}

        <div class="ms-auto flex flex-wrap items-center gap-2">
          {#if showProgress && downloadCount > 0}
            <div class="w-16">
              <Progress
                value={downloadValue}
                max={downloadCount}
                aria-label={t('common.downloadProgress')}
              />
            </div>
          {/if}
          <Tooltip>
            <TooltipTrigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="default"
                  size="sm"
                  disabled={updatingAll ||
                    capturingUpdateAllSafety ||
                    planningUpdateAll ||
                    busy ||
                    gameAddons.busy ||
                    nothingToUpdate}
                  aria-busy={capturingUpdateAllSafety || updatingAll || planningUpdateAll}
                  onclick={handleUpdateAll}
                >
                  {#if updatingAll || planningUpdateAll}
                    <Loader2Icon class="animate-spin" aria-hidden="true" />
                  {:else}
                    <ArrowUpToLineIcon aria-hidden="true" />
                  {/if}
                  {nothingToUpdate
                    ? t('gameDetails.updateAll.action')
                    : t('gameDetails.updateAll.actionCount', { count: totalUpdateCount })}
                </Button>
              {/snippet}
            </TooltipTrigger>
            <TooltipContent>
              {nothingToUpdate
                ? t('gameDetails.updateAll.upToDate')
                : t('gameDetails.updateAll.tooltip', { count: totalUpdateCount })}
            </TooltipContent>
          </Tooltip>

          {#if onOpenOperations}
            <Button
              variant="secondary"
              size="sm"
              onclick={onOpenOperations}
              onpointerenter={onPreloadOperations}
              onfocus={onPreloadOperations}
            >
              <HistoryIcon aria-hidden="true" />
              {t('operations.title')}
            </Button>
          {/if}

          <GameExecutablePopover {gameId} exe={gameExe} lockReason={executableLockReason} />
        </div>
      </div>

      <ScrollArea class="min-h-0 flex-1">
        <div class="grid gap-4 p-1">
          <GameFileSafetyRow assessment={fileSafety.assessment} />

          {#each vendorTabs as tab (tab.key)}
            <TabsContent value={tab.key} class="mt-0">
              <div class="grid gap-3">
                {#if tab.key === 'nvidia'}
                  {#if gameId && nvidia.nvapiAvailable}
                    <NvidiaProfileCard nvapi={nvidia} />
                  {/if}

                  {@const nonStreamline = tab.components.filter((c) => !isStreamline(c))}
                  {@const streamline = tab.components.filter(isStreamline)}

                  {#each nonStreamline as component (component.id)}
                    {@const group = getCandidateGroup(component.id)}
                    {@const dlssCard = dlssFamilyCard(component)}
                    {#if dlssCard && gameId}
                      <DlssComponentCard
                        {gameId}
                        {component}
                        {group}
                        family={dlssCard.family}
                        title={dlssCard.title}
                        {nvidia}
                        nvapiAvailable={nvidia.nvapiAvailable}
                        {busy}
                        onSwap={handleSwapWithSafety}
                        {onRollback}
                      />
                    {:else}
                      <VendorComponentCard
                        {component}
                        {group}
                        {busy}
                        onSwap={handleSwapWithSafety}
                        {onRollback}
                      />
                    {/if}
                  {/each}

                  {#if streamline.length > 0}
                    {@const groupsById = Object.fromEntries(
                      streamline.map((c) => [c.id, getCandidateGroup(c.id)] as const),
                    )}
                    <StreamlineComponentCard
                      components={streamline}
                      {groupsById}
                      coordinatedOptions={details?.streamline_candidate_options ?? []}
                      {busy}
                      onBulkSwap={handleBulkSwapWithSafety}
                      {onBulkRollback}
                    />
                  {/if}
                {:else}
                  {#each tab.components as component (component.id)}
                    {@const group = getCandidateGroup(component.id)}
                    <VendorComponentCard
                      {component}
                      {group}
                      {busy}
                      onSwap={handleSwapWithSafety}
                      {onRollback}
                    />
                  {/each}
                {/if}
              </div>
            </TabsContent>
          {/each}

          {#if tabs.addonsTab}
            <TabsContent value={ADDONS_TAB_VALUE} class="mt-0">
              <div class="grid grid-cols-[repeat(auto-fit,minmax(min(100%,50rem),1fr))] gap-3">
                {#if gameAddons.isEnabled('renodx')}
                  <RenoDxCard
                    {gameId}
                    busy={exclusiveBusy}
                    store={renodx}
                    {onOpenRenoDxSettings}
                    {onPreloadRenoDxSettings}
                  />
                {/if}
                {#if gameAddons.isEnabled('luma')}
                  <LumaCard {gameId} busy={exclusiveBusy} {launcher} store={luma} />
                {/if}
              </div>
            </TabsContent>
          {/if}
        </div>
      </ScrollArea>
    </Tabs>
  {/if}
</section>

<D3d12ExecutableConfirmDialog
  open={updateConfirmOpen}
  busy={updatingAll}
  actions={preparedUpdateExecutableActions}
  reason="update_all"
  onOpenChange={(open: boolean) => {
    updateAllWorkflow.setConfirmationOpen(open);
  }}
  onConfirm={() => void updateAllWorkflow.confirm()}
/>

<DeveloperModeRequirementDialog
  open={updateAllWorkflow.developerModeOpen}
  blocker={updateAllWorkflow.developerModeBlocker}
  retrying={updateAllWorkflow.developerModeRetrying}
  stillDisabledAfterRetry={updateAllWorkflow.developerModeStillDisabledAfterRetry}
  onOpenChange={(open: boolean) => {
    if (!open) {
      updateAllWorkflow.cancelDeveloperMode();
    }
  }}
  onRetry={() => void updateAllWorkflow.retryDeveloperMode()}
/>
