<script lang="ts">
  import { areSameGameIds, type GameDetails } from '@entities/game';
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
  import {
    publishCommandErrorNotification,
    publishPresentedErrorNotification,
  } from '@shared/notifications';
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
  import { createMutationConfirmationCoordinator } from '../model/create-mutation-confirmation.svelte';
  import { createUpdateAllWorkflow } from '../model/create-update-all-workflow.svelte';
  import { createNvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import { createGameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type { MutationSafetyTokens } from '@entities/addon';
  import type { UpdateAllOwner } from '../model/run-update-all';
  import {
    d3d12PlanFingerprint,
    type PreparedSwapPresentation,
    type SwapRequest,
  } from '../model/swap-request';
  import { resolveExecutableLockReason } from '../model/game-executable-lock';
  import DeveloperModeRequirementDialog from './DeveloperModeRequirementDialog.svelte';
  import { onDestroy, untrack } from 'svelte';
  import GameDetailsToolbar from './GameDetailsToolbar.svelte';
  import GameDetailsTabsContent from './GameDetailsTabsContent.svelte';
  import FileSafetyConfirmationDialog from './FileSafetyConfirmationDialog.svelte';
  import { createNvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';

  type Props = {
    details?: GameDetails | null;
    busy?: boolean;
    onSwap?: SwapHandler;
    onRollback?: RollbackHandler;
    onBulkSwap?: BulkSwapHandler;
    onBulkRollback?: BulkRollbackHandler;
    onOpenOperations?: () => void;
    onOpenGameDetails?: (gameId: string) => void | Promise<void>;
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
    onOpenGameDetails = () => undefined,
    onPreloadOperations = () => undefined,
    onOpenRenoDxSettings = () => undefined,
    onPreloadRenoDxSettings = () => undefined,
    onGameDetailsInvalidate = () => undefined,
  }: Props = $props();

  const tabs = $derived(createGameDetailsTabs(details));
  const vendorTabs = $derived(tabs.vendorTabs);
  const gameId = $derived(details?.game.identity.id ?? null);
  const installPath = $derived(details?.game.install_path ?? null);
  const executableLockReason = $derived(resolveExecutableLockReason(details?.components ?? []));
  // The game's launcher, for add-on launch-argument instructions.
  const launcher = $derived(details?.game.identity.launcher ?? '');

  const fileSafety = createFileSafetyContext({
    getGameId: () => gameId,
    getInstallPath: () => installPath,
  });
  const mutationConfirmation = createMutationConfirmationCoordinator({
    getGameId: () => gameId,
    getInstallPath: () => installPath,
    captureFreshContext: (scope) => fileSafety.captureFreshContext(scope),
    isCurrentCapture: (captured) => fileSafety.isCurrentCapture(captured),
    invalidatePendingCapture: () => {
      fileSafety.invalidatePendingCapture();
    },
  });
  // Update All deliberately keeps one captured assessment for every step. A
  // refreshed context is used by the next user action, while this run stops on
  // the stale token instead of silently switching authorization mid-batch.
  let activeUpdateAllCapture = $state.raw<{
    owner: UpdateAllOwner;
    scope: FileSafetyScope;
    tokens: MutationSafetyTokens;
  } | null>(null);
  let capturingUpdateAllSafety = $state(false);

  function updateAllOwnerMismatch(): DesktopCommandError {
    return DesktopCommandError.fromDto({ code: 'safety_context_scope_mismatch' });
  }

  async function requirePageSafetyTokens(
    requestedGameId: string,
    scope: FileSafetyScope,
  ): Promise<MutationSafetyTokens | null> {
    const activeCapture = activeUpdateAllCapture;
    if (activeCapture) {
      const { owner, tokens } = activeCapture;
      if (
        !gameId ||
        !areSameGameIds(requestedGameId, gameId) ||
        !areSameGameIds(owner.gameId, requestedGameId) ||
        !areSameGameIds(owner.gameId, gameId) ||
        owner.installPath !== installPath
      ) {
        throw updateAllOwnerMismatch();
      }
      if (
        scope === 'game_and_shared' &&
        (activeCapture.scope !== 'game_and_shared' || !tokens.sharedVulkanContextToken)
      ) {
        throw updateAllOwnerMismatch();
      }
      return tokens;
    }
    if (!gameId || !areSameGameIds(requestedGameId, gameId)) {
      return null;
    }
    return mutationConfirmation.requestTokens({ scope });
  }

  const gameAddons = createGameAddonsContext({
    getGameId: () => gameId,
    getCapabilities: () => tabs.addonsTab?.capabilities ?? [],
    onGameDetailsInvalidate: (id) => onGameDetailsInvalidate(id),
    requireSafetyTokens: (id, scope) => requirePageSafetyTokens(id, scope),
  });
  const { renodx, luma, optiscaler } = gameAddons.stores;

  // The single "update everything to its latest version" action. Spans every
  // vendor (NVIDIA/AMD/Intel) plus the Streamline bundle, RenoDX, and Luma, not
  // just the active tab, and reuses the existing bulk-swap path.
  const updatePlan = $derived(buildUpdateAllToLatestPlan(details));
  const totalUpdateCount = $derived(updatePlan.updateCount + gameAddons.updateCount);
  const nothingToUpdate = $derived(totalUpdateCount === 0);

  const updateAllWorkflow = createUpdateAllWorkflow({
    getGameId: () => gameId,
    getInstallPath: () => installPath,
    getPlan: () => updatePlan,
    getAddonUpdates: () => gameAddons.addonUpdates,
    hasUpdates: () => !nothingToUpdate,
    isBusy: () => busy || gameAddons.busy,
    onBulkSwap: (items) => handleBulkSwapWithSafety(items),
    onPreparationError: reportUpdateAllPreparationError,
    onError: reportUpdateAllError,
  });
  const updatingAll = $derived(updateAllWorkflow.updating);
  const planningUpdateAll = $derived(updateAllWorkflow.planning);
  const preparedUpdateBatch = $derived(updateAllWorkflow.preparedBatch);
  const pendingDownloadIds = $derived(updateAllWorkflow.pendingDownloadIds);
  let updateAllOwnerKey: string | null | undefined;
  let updateAllOwnerRevision = 0;

  type CapturedUpdateAllOwner = Readonly<{
    gameId: string;
    installPath: string | null;
    revision: number;
  }>;

  function captureUpdateAllOwner(): CapturedUpdateAllOwner | null {
    return gameId ? Object.freeze({ gameId, installPath, revision: updateAllOwnerRevision }) : null;
  }

  function isCurrentUpdateAllOwner(owner: CapturedUpdateAllOwner): boolean {
    return (
      owner.revision === updateAllOwnerRevision &&
      gameId !== null &&
      areSameGameIds(gameId, owner.gameId) &&
      installPath === owner.installPath
    );
  }

  $effect(() => {
    const currentOwnerKey = JSON.stringify([gameId, installPath]);
    if (updateAllOwnerKey === undefined) {
      updateAllOwnerKey = currentOwnerKey;
      return;
    }
    if (currentOwnerKey !== updateAllOwnerKey) {
      updateAllOwnerKey = currentOwnerKey;
      updateAllOwnerRevision += 1;
      untrack(() => {
        updateAllWorkflow.invalidatePending();
        mutationConfirmation.cancel();
      });
    }
  });

  onDestroy(() => {
    updateAllWorkflow.destroy();
    mutationConfirmation.destroy();
    gameAddons.destroy();
    fileSafety.destroy();
    nvidiaProfile.clear();
  });
  const safetyConfirmationOpen = $derived(mutationConfirmation.pending !== null);
  const safetyGateActive = $derived(mutationConfirmation.requesting || safetyConfirmationOpen);
  const exclusiveBusy = $derived(
    busy || gameAddons.busy || updatingAll || planningUpdateAll || safetyGateActive,
  );
  const showProgress = $derived(updatingAll && pendingDownloadIds.length > 0);
  const downloadCount = $derived(pendingDownloadIds.length);
  const downloadValue = $derived(showProgress ? sumDownloadFractions(pendingDownloadIds) : 0);

  function updateAllSafetyScope(addonUpdates: typeof gameAddons.addonUpdates): FileSafetyScope {
    const includesRenoDx = addonUpdates.some(({ step }) => step === 'renodx');
    if (!includesRenoDx) {
      return 'game';
    }
    return renodx.state?.status === 'installed' && renodx.state.host_kind === 'proxy'
      ? 'game'
      : 'game_and_shared';
  }

  async function confirmAndRunPreparedUpdateAll(): Promise<void> {
    const batch = updateAllWorkflow.preparedBatch;
    if (!batch) {
      return;
    }

    const owner = Object.freeze({ gameId: batch.gameId, installPath: batch.installPath });
    const scope = updateAllSafetyScope(batch.addonUpdates);
    const tokens = await mutationConfirmation.requestTokens({
      scope,
      actions: updateAllWorkflow.confirmationActions,
      isCurrent: () =>
        gameId !== null &&
        areSameGameIds(gameId, owner.gameId) &&
        installPath === owner.installPath &&
        updateAllWorkflow.isCurrentPreparedBatch(batch) &&
        updateAllSafetyScope(batch.addonUpdates) === scope,
    });
    if (!tokens) {
      updateAllWorkflow.invalidatePending();
      return;
    }

    const capture = Object.freeze({ owner, scope, tokens });
    activeUpdateAllCapture = capture;
    try {
      await updateAllWorkflow.confirm();
    } finally {
      if (activeUpdateAllCapture === capture) {
        activeUpdateAllCapture = null;
      }
    }
  }

  async function handleUpdateAll(): Promise<void> {
    if (
      !gameId ||
      capturingUpdateAllSafety ||
      updatingAll ||
      planningUpdateAll ||
      updateAllWorkflow.developerModeOpen ||
      safetyGateActive ||
      gameAddons.busy ||
      busy ||
      nothingToUpdate
    ) {
      return;
    }
    const requestOwner = captureUpdateAllOwner();
    if (!requestOwner) {
      return;
    }
    capturingUpdateAllSafety = true;
    try {
      // Preflight first so the page-owned dialog can show any executable plan
      // and the current file-risk notice in one confirmation.
      await updateAllWorkflow.start();
      await confirmAndRunPreparedUpdateAll();
    } catch (error) {
      updateAllWorkflow.invalidatePending();
      if (isCurrentUpdateAllOwner(requestOwner)) {
        reportUpdateAllPreparationError(error);
      }
    } finally {
      capturingUpdateAllSafety = false;
    }
  }

  async function retryUpdateAllDeveloperMode(): Promise<void> {
    if (
      !updateAllWorkflow.developerModeOpen ||
      capturingUpdateAllSafety ||
      updatingAll ||
      planningUpdateAll ||
      safetyGateActive ||
      gameAddons.busy ||
      busy
    ) {
      return;
    }
    const requestOwner = captureUpdateAllOwner();
    if (!requestOwner) {
      return;
    }
    capturingUpdateAllSafety = true;
    try {
      await updateAllWorkflow.retryDeveloperMode();
      await confirmAndRunPreparedUpdateAll();
    } catch (error) {
      updateAllWorkflow.invalidatePending();
      if (isCurrentUpdateAllOwner(requestOwner)) {
        reportUpdateAllPreparationError(error);
      }
    } finally {
      capturingUpdateAllSafety = false;
    }
  }

  function isCurrentSwap(request: SwapRequest, presentation?: PreparedSwapPresentation): boolean {
    if (!gameId || !installPath || !details || !areSameGameIds(details.game.identity.id, gameId)) {
      return false;
    }
    const component = details.components.find((item) => item.id === request.componentId);
    const candidate = details.candidate_groups
      .find((group) => group.component_id === request.componentId)
      ?.candidates.find((item) => item.artifact_id === request.artifactId);
    if (!component || !candidate) {
      return false;
    }
    if (!presentation) {
      return true;
    }
    const { owner } = presentation;
    return (
      areSameGameIds(owner.gameId, gameId) &&
      owner.installPath === installPath &&
      owner.componentId === request.componentId &&
      owner.artifactId === request.artifactId &&
      owner.planFingerprint ===
        d3d12PlanFingerprint(component.d3d12_executable_status, candidate.d3d12_executable_action)
    );
  }

  async function handleSwapWithSafety(
    request: Parameters<SwapHandler>[0],
    presentation?: PreparedSwapPresentation,
  ): Promise<void> {
    if (!gameId) {
      throw DesktopCommandError.fromDto({ code: 'safety_context_missing' });
    }
    const tokens = await mutationConfirmation.requestTokens({
      scope: 'game',
      actions: presentation ? [presentation.action] : [],
      isCurrent: () => isCurrentSwap(request, presentation),
    });
    if (!tokens) {
      return;
    }
    await onSwap({ ...request, gameContextToken: tokens.gameContextToken });
  }

  async function handleBulkSwapWithSafety(items: readonly SwapRequest[]): Promise<void> {
    if (!gameId) {
      throw DesktopCommandError.fromDto({ code: 'safety_context_missing' });
    }
    const tokens = await requirePageSafetyTokens(gameId, 'game');
    if (!tokens) {
      return;
    }
    await onBulkSwap(
      items.map((item) => ({
        ...item,
        gameContextToken: tokens.gameContextToken,
      })),
    );
  }

  async function runStandaloneMutation(mutation: () => Promise<void>): Promise<void> {
    try {
      await mutation();
    } catch (error) {
      publishCommandErrorNotification(error);
      reportClientError('game_details_standalone_mutation', error);
    }
  }

  function handleStandaloneSwapWithSafety(
    request: Parameters<SwapHandler>[0],
    presentation?: PreparedSwapPresentation,
  ): Promise<void> {
    return runStandaloneMutation(() => handleSwapWithSafety(request, presentation));
  }

  function handleStandaloneBulkSwapWithSafety(items: readonly SwapRequest[]): Promise<void> {
    return runStandaloneMutation(() => handleBulkSwapWithSafety(items));
  }

  function reportUpdateAllError(error: unknown): void {
    const failureCount = error instanceof UpdateAllError ? error.failures.length : 1;
    const primaryFailure = error instanceof UpdateAllError ? error.failures[0] : undefined;
    const primaryError =
      error instanceof UpdateAllError ? (error.failures[0]?.error ?? error) : error;
    if (primaryFailure?.reportedByStore === true && isFileSafetyContextError(primaryError)) {
      reportClientError('update_all_workflow', primaryError);
      return;
    }
    publishPresentedErrorNotification(
      t('gameDetails.updateAll.partialFailure', { count: failureCount }),
      primaryError,
    );
    reportClientError('update_all_workflow', primaryError);
  }

  function reportUpdateAllPreparationError(error: unknown): void {
    publishPresentedErrorNotification(t('gameDetails.updateAll.prepareFailed'), error);
    reportClientError('update_all_preparation', error);
  }

  const hasNvidiaTab = $derived(vendorTabs.some((tab) => tab.key === 'nvidia'));
  const isWindowsGame = $derived(details?.game.platform === 'Windows');

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
        isWindowsGame ? nvidiaProfile.reload(id) : Promise.resolve(),
        onGameDetailsInvalidate(id),
      ]);
    },
  });
  const nvidiaProfile = createNvapiProfileContext(async (id) => {
    await Promise.all([
      gameExe.reload(id),
      hasNvidiaTab ? nvidia.reload(id) : Promise.resolve(),
      onGameDetailsInvalidate(id),
    ]);
  });
  const profileSelectionBlockReason = $derived.by(() => {
    if (!isWindowsGame) {
      return null;
    }
    if (nvidiaProfile.loading) {
      return 'checking' as const;
    }
    if (nvidiaProfile.loadError !== null || nvidiaProfile.status?.state === 'error') {
      return 'unverified' as const;
    }
    if (nvidiaProfile.status === null) {
      return 'checking' as const;
    }
    return null;
  });
  let pageHeading = $state<HTMLHeadingElement | null>(null);

  function focusPageHeading(): void {
    pageHeading?.focus({ preventScroll: true });
  }

  $effect(() => {
    const id = gameId;
    const windowsGame = isWindowsGame;

    untrack(() => {
      if (!id) {
        gameExe.clear();
        nvidiaProfile.clear();
        return;
      }

      if (!windowsGame) {
        nvidiaProfile.clear();
        void gameExe.reload(id);
        return;
      }

      void Promise.all([gameExe.reload(id), nvidiaProfile.reload(id)]);
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
  <h1 id="game-details-title" bind:this={pageHeading} class="sr-only" tabindex="-1">
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
    <Tabs bind:value={selectedTab} class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
      <GameDetailsToolbar
        title={details.game.identity.title}
        {vendorTabs}
        hasAddonsTab={tabs.addonsTab !== null}
        {gameId}
        exe={gameExe}
        lockReason={executableLockReason}
        ownedBindingPath={nvidiaProfile.ownedBindingPath}
        {profileSelectionBlockReason}
        onMoveProfile={(path: string, selectAutomatically: boolean) =>
          nvidiaProfile.moveTo(gameId, path, selectAutomatically)}
        {showProgress}
        {downloadCount}
        {downloadValue}
        {updatingAll}
        {capturingUpdateAllSafety}
        {planningUpdateAll}
        busy={exclusiveBusy}
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
          installPath={details.game.install_path}
          profile={nvidiaProfile}
          {onOpenGameDetails}
          onRecoveryDeleteComplete={focusPageHeading}
          {vendorTabs}
          hasAddonsTab={tabs.addonsTab !== null}
          {nvidia}
          busy={busy || safetyGateActive}
          {exclusiveBusy}
          {launcher}
          {renodx}
          {luma}
          {optiscaler}
          renodxEnabled={gameAddons.isEnabled('renodx')}
          lumaEnabled={gameAddons.isEnabled('luma')}
          optiscalerEnabled={gameAddons.isEnabled('optiscaler')}
          onSwap={handleStandaloneSwapWithSafety}
          {onRollback}
          onBulkSwap={handleStandaloneBulkSwapWithSafety}
          {onBulkRollback}
          {onOpenRenoDxSettings}
          {onPreloadRenoDxSettings}
        />
      </ScrollArea>
    </Tabs>
  {/if}
</section>

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
  onRetry={() => void retryUpdateAllDeveloperMode()}
/>

<FileSafetyConfirmationDialog
  notice={mutationConfirmation.pending?.notice ?? null}
  actions={mutationConfirmation.pending?.actions ?? []}
  isUpdateAll={preparedUpdateBatch !== null}
  onCancel={() => {
    mutationConfirmation.pending?.resolve(false);
    if (preparedUpdateBatch) {
      updateAllWorkflow.invalidatePending();
    }
  }}
  onConfirm={(rememberGeneralWarning: boolean) => {
    mutationConfirmation.pending?.resolve(true, rememberGeneralWarning);
  }}
/>
