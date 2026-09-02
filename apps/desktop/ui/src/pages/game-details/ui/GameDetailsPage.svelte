<script lang="ts">
  import type { GameDetails } from '@entities/game';
  import {
    createGameDetailsTabs,
    DLSS_FAMILY_CARDS,
    reconcileGameDetailsTabValue,
  } from '../model/game-details-tabs';
  import { Tabs, Card, CardContent, CardDescription, CardTitle, ScrollArea } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { track } from '@shared/reactivity';
  import { DesktopCommandError, isFileSafetyContextError, reportClientError } from '@shared/errors';
  import { sumDownloadFractions } from '@shared/lib';
  import { publishPresentedErrorNotification } from '@shared/notifications';
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
  import D3d12ExecutableConfirmDialog from './D3d12ExecutableConfirmDialog.svelte';
  import DeveloperModeRequirementDialog from './DeveloperModeRequirementDialog.svelte';
  import { areSameGameIds } from '@entities/game';
  import { onDestroy, untrack } from 'svelte';
  import GameDetailsToolbar from './GameDetailsToolbar.svelte';
  import GameDetailsTabsContent from './GameDetailsTabsContent.svelte';

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
    track(dlssFingerprint);

    untrack(() => {
      if (!id || !shouldLoad) {
        nvidia.clear();
        return;
      }

      void nvidia.reload(id);
    });
  });
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
      <GameDetailsToolbar
        title={details.game.identity.title}
        {vendorTabs}
        hasAddonsTab={tabs.addonsTab !== null}
        {gameId}
        exe={gameExe}
        lockReason={executableLockReason}
        {showProgress}
        {downloadCount}
        {downloadValue}
        {updatingAll}
        {capturingUpdateAllSafety}
        {planningUpdateAll}
        {busy}
        addonsBusy={gameAddons.busy}
        {nothingToUpdate}
        {totalUpdateCount}
        onUpdateAll={handleUpdateAll}
        {onOpenOperations}
        {onPreloadOperations}
      />

      <ScrollArea class="min-h-0 flex-1">
        <GameDetailsTabsContent
          {details}
          {gameId}
          {vendorTabs}
          hasAddonsTab={tabs.addonsTab !== null}
          assessment={fileSafety.assessment}
          {nvidia}
          {busy}
          {exclusiveBusy}
          {launcher}
          {renodx}
          {luma}
          renodxEnabled={gameAddons.isEnabled('renodx')}
          lumaEnabled={gameAddons.isEnabled('luma')}
          onSwap={handleSwapWithSafety}
          {onRollback}
          onBulkSwap={handleBulkSwapWithSafety}
          {onBulkRollback}
          {onOpenRenoDxSettings}
          {onPreloadRenoDxSettings}
        />
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
