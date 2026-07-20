<script lang="ts">
  import type { GameCandidateGroup, GameDetails, GameGraphicsComponent } from '@entities/game';
  import {
    createVendorTabs,
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
  import { describeCommandErrorTechnical } from '@shared/api';
  import { sumDownloadFractions } from '@shared/lib';
  import { publishErrorNotification } from '@shared/notifications';
  import type { SettingFamily } from '@features/nvapi-settings';
  import { RenoDxCard, createRenoDxStore } from '@features/renodx';
  import { LumaCard, createLumaStore } from '@features/luma';
  import type {
    SwapHandler,
    RollbackHandler,
    BulkSwapHandler,
    BulkRollbackHandler,
  } from '../model/create-game-details-page-model';
  import { createExclusiveAddonStores } from '@entities/addon';
  import { buildUpdateAllToLatestPlan } from '../model/update-all-to-latest';
  import { runUpdateAll, UpdateAllError } from '../model/run-update-all';
  import { createNvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import { createGameExecutableContext } from '../model/create-game-executable-context.svelte';
  import GameExecutablePopover from './GameExecutablePopover.svelte';
  import NvidiaProfileCard from './NvidiaProfileCard.svelte';
  import DlssComponentCard from './DlssComponentCard.svelte';
  import StreamlineComponentCard from './StreamlineComponentCard.svelte';
  import VendorComponentCard from './VendorComponentCard.svelte';
  import { untrack } from 'svelte';

  type Props = {
    details?: GameDetails | null;
    busy?: boolean;
    /**
     * Whether the process is running elevated; controls NVAPI write
     * affordances (setting Select / revert buttons) inside the DLSS cards.
     */
    isElevated?: boolean;
    onSwap?: SwapHandler;
    onRollback?: RollbackHandler;
    onBulkSwap?: BulkSwapHandler;
    onBulkRollback?: BulkRollbackHandler;
    onOpenOperations?: () => void;
    onOpenRenoDxSettings?: () => void;
    onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  };

  const {
    details = null,
    busy = false,
    isElevated = true,
    onSwap = () => undefined,
    onRollback = () => undefined,
    onBulkSwap = () => undefined,
    onBulkRollback = () => undefined,
    onOpenOperations,
    onOpenRenoDxSettings = () => undefined,
    onGameDetailsInvalidate = () => undefined,
  }: Props = $props();

  const vendorTabs = $derived(createVendorTabs(details));
  // The "Other" tab always hosts the RenoDX and Luma cards for the selected
  // game. We render them unconditionally so the full availability logic inside
  // each card can detect tracked installs, unmanaged files, orphans, etc.
  // (addon_capabilities from the list is used for badges/filters, not to hide
  // detail cards).
  const OTHER_TAB = 'other';
  const gameId = $derived(details?.game.identity.id ?? null);
  // The game's launcher, for Luma's launcher-aware launch-args callout.
  const launcher = $derived(details?.game.identity.launcher ?? '');

  // RenoDX and Luma are mutually exclusive per game. Stores are created once;
  // the page loads every store for the selected game — not gated on
  // addon_capabilities from the list — so each card can detect tracked installs,
  // unmanaged files, orphans, and blocked_by_other_addon state. Successful
  // install/uninstall mutations reload peer stores via createExclusiveAddonStores.
  //
  // Add another tool by adding a factory entry here + placing its card in the
  // OTHER_TAB template below.
  const {
    stores: { renodx, luma },
    list: addonStores,
  } = createExclusiveAddonStores(
    {
      renodx: ({ onExclusivityChange }) => createRenoDxStore({ onExclusivityChange }),
      luma: ({ onExclusivityChange }) =>
        createLumaStore({ onExclusivityChange, onGameDetailsInvalidate }),
    },
    { shouldReloadPeers: (id) => !!gameId && id === gameId },
  );

  // Update availability is read from the list of stores for bulk operations.

  // The single "update everything to its latest version" action. Spans every
  // vendor (NVIDIA/AMD/Intel) plus the Streamline bundle, RenoDX, and Luma, not
  // just the active tab, and reuses the existing bulk-swap path.
  const updatePlan = $derived(buildUpdateAllToLatestPlan(details));
  const totalUpdateCount = $derived(
    updatePlan.updateCount + addonStores.filter((s) => s.updateAvailable).length,
  );
  const nothingToUpdate = $derived(totalUpdateCount === 0);

  // This workflow owns its lifecycle through try/finally. The aggregate bar
  // tracks only library artifacts that actually download; add-on-only runs keep
  // the button spinner/aria-busy active without rendering an empty progress bar.
  let updatingAll = $state(false);
  let pendingDownloadIds = $state<string[]>([]);
  const anyAddonBusy = $derived(addonStores.some((store) => store.busy));
  // Shared exclusive gate for Luma/RenoDX cards (peer mutations + Update-all).
  const exclusiveBusy = $derived(busy || anyAddonBusy || updatingAll);
  const showProgress = $derived(updatingAll && pendingDownloadIds.length > 0);
  const downloadCount = $derived(pendingDownloadIds.length);
  const downloadValue = $derived(showProgress ? sumDownloadFractions(pendingDownloadIds) : 0);

  async function handleUpdateAll() {
    if (updatingAll || busy || anyAddonBusy || nothingToUpdate) {
      return;
    }

    const items = [...updatePlan.items];
    const id = gameId;
    const eligibleAddonUpdates = [
      { step: 'renodx' as const, store: renodx },
      { step: 'luma' as const, store: luma },
    ].filter(({ store }) => store.updateAvailable);
    updatingAll = true;
    pendingDownloadIds = items.filter((item) => !item.isDownloaded).map((item) => item.artifactId);

    try {
      await runUpdateAll({
        items,
        gameId: id,
        addonUpdates: eligibleAddonUpdates,
        onBulkSwap,
      });
    } catch (error) {
      const failureCount = error instanceof UpdateAllError ? error.failures.length : 1;
      const technical = describeCommandErrorTechnical(
        error instanceof UpdateAllError ? error.failures[0].error : error,
      );
      publishErrorNotification(
        t('gameDetails.updateAll.partialFailure', { count: failureCount }),
        technical,
      );
      console.warn('Update-all workflow failed after attempting eligible updates', error);
    } finally {
      updatingAll = false;
      pendingDownloadIds = [];
    }
  }

  const hasNvidiaTab = $derived(vendorTabs.some((tab) => tab.key === 'nvidia'));

  // The page owns the addon stores' load (the cards render from them), so their
  // update status feeds "Update all". We load all for the selected game so full
  // on-disk / availability detection can happen (including unmanaged or orphaned
  // installs not present in caps).
  $effect(() => {
    const id = gameId;

    if (!id) {
      return;
    }

    untrack(() => {
      for (const store of addonStores) {
        void store.load(id);
      }
    });
  });

  // The active vendor tab is user-controlled state, not derived: a post-swap
  // details reload re-derives `tabs`, and a hardcoded `value={tabs[0].key}`
  // would snap the user back to the first vendor (NVIDIA) every time. Reconcile
  // only when the set of available tabs changes — keep the current selection if
  // that vendor still has components, otherwise fall back to the first tab.
  let selectedVendor = $state('');
  $effect(() => {
    // Vendor tabs come and go with the game's components. The Other tab (RenoDX + Luma)
    // is always present for the selected game.
    const keys = [...vendorTabs.map((tab) => tab.key as string), OTHER_TAB];
    untrack(() => {
      if (!keys.includes(selectedVendor)) {
        selectedVendor = keys[0];
      }
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
  const nvidia = createNvidiaDriverContext({ isElevated: () => isElevated });

  // The executable is a game-level identity feeding both the NVIDIA profile target
  // and the RenoDX install location, so it lives above the tabs in its own context.
  // Changing it re-reads the NVIDIA settings (they key off the profile's exe).
  const gameExe = createGameExecutableContext({
    onChange: (id) => {
      if (hasNvidiaTab) {
        void nvidia.reload(id);
      }
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
    component: GameGraphicsComponent,
  ): { family: SettingFamily; title: string } | null {
    return DLSS_FAMILY_CARDS[component.technology] ?? null;
  }

  function isStreamline(component: GameGraphicsComponent): boolean {
    return component.technology === NVIDIA_STREAMLINE_TECHNOLOGY;
  }
</script>

<section class="flex h-full min-h-0 flex-col overflow-hidden">
  {#if !details}
    <Card>
      <CardContent>
        <CardTitle>{t('gameDetails.noGameSelected.title')}</CardTitle>
        <CardDescription>
          {t('gameDetails.noGameSelected.description')}
        </CardDescription>
      </CardContent>
    </Card>
  {:else if gameId}
    <!-- Match Settings/Libraries: sticky tab chrome, scroll only inside TabsContent. -->
    <Tabs bind:value={selectedVendor} class="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
      <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <TabsList>
          {#each vendorTabs as tab (tab.key)}
            <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
          {/each}
          <TabsTrigger value={OTHER_TAB}>{t('gameDetails.otherTab')}</TabsTrigger>
        </TabsList>

        <div class="flex flex-wrap items-center gap-2">
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
              <Button
                variant="default"
                size="sm"
                disabled={updatingAll || busy || anyAddonBusy || nothingToUpdate}
                aria-busy={updatingAll}
                onclick={handleUpdateAll}
              >
                {#if updatingAll}
                  <Loader2Icon class="animate-spin" aria-hidden="true" />
                {:else}
                  <ArrowUpToLineIcon aria-hidden="true" />
                {/if}
                {nothingToUpdate
                  ? t('gameDetails.updateAll.action')
                  : t('gameDetails.updateAll.actionCount', { count: totalUpdateCount })}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {nothingToUpdate
                ? t('gameDetails.updateAll.upToDate')
                : t('gameDetails.updateAll.tooltip', { count: totalUpdateCount })}
            </TooltipContent>
          </Tooltip>

          {#if onOpenOperations}
            <Button variant="secondary" size="sm" onclick={onOpenOperations}>
              <HistoryIcon aria-hidden="true" />
              {t('operations.title')}
            </Button>
          {/if}

          <GameExecutablePopover {gameId} exe={gameExe} />
        </div>
      </div>

      {#each vendorTabs as tab (tab.key)}
        <TabsContent value={tab.key} class="min-h-0 flex-1 overflow-hidden">
          <ScrollArea class="h-full">
            <div class="grid gap-3 p-1">
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
                      {onSwap}
                      {onRollback}
                    />
                  {:else}
                    <VendorComponentCard {component} {group} {busy} {onSwap} {onRollback} />
                  {/if}
                {/each}

                {#if streamline.length > 0}
                  {@const groupsById = Object.fromEntries(
                    streamline.map((c) => [c.id, getCandidateGroup(c.id)] as const),
                  )}
                  <StreamlineComponentCard
                    components={streamline}
                    {groupsById}
                    {busy}
                    {onBulkSwap}
                    {onBulkRollback}
                  />
                {/if}
              {:else}
                {#each tab.components as component (component.id)}
                  {@const group = getCandidateGroup(component.id)}
                  <VendorComponentCard {component} {group} {busy} {onSwap} {onRollback} />
                {/each}
              {/if}
            </div>
          </ScrollArea>
        </TabsContent>
      {/each}

      <TabsContent value={OTHER_TAB} class="min-h-0 flex-1 overflow-hidden">
        <ScrollArea class="h-full">
          <div class="grid gap-3 p-1">
            <RenoDxCard {gameId} busy={exclusiveBusy} store={renodx} {onOpenRenoDxSettings} />
            <LumaCard {gameId} busy={exclusiveBusy} {launcher} store={luma} />
          </div>
        </ScrollArea>
      </TabsContent>
    </Tabs>
  {/if}
</section>
