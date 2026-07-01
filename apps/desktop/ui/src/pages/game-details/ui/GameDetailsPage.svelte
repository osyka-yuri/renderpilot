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
  import { sumDownloadFractions, BatchDownloadProgressBar } from '@entities/library';
  import type { SettingFamily } from '@features/nvapi-settings';
  import { RenoDxCard, createRenoDxStore } from '@features/renodx';
  import type {
    SwapHandler,
    RollbackHandler,
    BulkSwapHandler,
    BulkRollbackHandler,
  } from '../model/create-game-details-page-model';
  import { buildUpdateAllToLatestPlan } from '../model/update-all-to-latest';
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
  }: Props = $props();

  const vendorTabs = $derived(createVendorTabs(details));
  // The "Other" tab (currently the RenoDX/HDR card) is fixed, always present
  // alongside the vendor tabs.
  const OTHER_TAB = 'other';

  // RenoDX/HDR is lifted to the page so it folds into "Update all" (single source
  // of truth for the count + the run); the card renders from this same store.
  const renodx = createRenoDxStore();

  // The single "update everything to its latest version" action. Spans every
  // vendor (NVIDIA/AMD/Intel) plus the Streamline bundle and RenoDX, not just the
  // active tab, and reuses the existing bulk-swap path.
  const updatePlan = $derived(buildUpdateAllToLatestPlan(details));
  const totalUpdateCount = $derived(updatePlan.updateCount + (renodx.updateAvailable ? 1 : 0));
  const nothingToUpdate = $derived(totalUpdateCount === 0);

  // "Update all" progress, shown only while a run initiated by THIS button is in
  // flight. `busy` is global (any exclusive op), so gate on `updatingAll` too.
  // Both reset once `busy` settles. The aggregate bar tracks only the artifacts
  // that actually download (uncached), advancing monotonically 0→100% across the
  // whole batch — like the libraries "Download all" button.
  let updatingAll = $state(false);
  let pendingDownloadIds = $state<string[]>([]);
  $effect(() => {
    if (!busy && !renodx.busy) {
      updatingAll = false;
      pendingDownloadIds = [];
    }
  });

  const showProgress = $derived(updatingAll && busy);
  const downloadCount = $derived(pendingDownloadIds.length);
  const downloadValue = $derived(showProgress ? sumDownloadFractions(pendingDownloadIds) : 0);

  function handleUpdateAll() {
    if (busy || renodx.busy || nothingToUpdate) {
      return;
    }
    updatingAll = true;
    pendingDownloadIds = updatePlan.items
      .filter((item) => !item.isDownloaded)
      .map((item) => item.artifactId);
    void runUpdateAll();
  }

  // Library components run through the existing bulk-swap path; RenoDX (add-on +
  // ReShade host updates via its own store, in the same single action.
  async function runUpdateAll() {
    if (updatePlan.items.length > 0) {
      await onBulkSwap(updatePlan.items);
    }
    const id = gameId;
    if (id && renodx.updateAvailable) {
      await renodx.update(id);
    }
  }

  const hasNvidiaTab = $derived(vendorTabs.some((tab) => tab.key === 'nvidia'));
  const gameId = $derived(details?.game.identity.id ?? null);

  // The page owns the RenoDX store's load (the card renders from it), so its
  // update status feeds "Update all". Reloads when the selected game changes.
  $effect(() => {
    const id = gameId;
    if (id) {
      void renodx.load(id);
    }
  });

  // The active vendor tab is user-controlled state, not derived: a post-swap
  // details reload re-derives `tabs`, and a hardcoded `value={tabs[0].key}`
  // would snap the user back to the first vendor (NVIDIA) every time. Reconcile
  // only when the set of available tabs changes — keep the current selection if
  // that vendor still has components, otherwise fall back to the first tab.
  let selectedVendor = $state('');
  $effect(() => {
    // Vendor tabs come and go with the game's components; the RenoDX tab is
    // always present (and last). Keep the current selection if it still exists,
    // otherwise fall back to the first available tab.
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
    if (id) {
      void gameExe.reload(id);
    } else {
      gameExe.clear();
    }
  });

  $effect(() => {
    // Reactive reads inside the effect determine when it re-runs:
    //   - gameId / hasNvidiaTab: standard load/teardown
    //   - dlssFingerprint:       re-load after any DLSS DLL swap
    void dlssFingerprint;
    if (hasNvidiaTab && gameId) {
      void nvidia.reload(gameId);
    } else {
      nvidia.clear();
    }
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

<ScrollArea class="h-full min-h-0">
  <section class="grid gap-4 p-1">
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
      <Tabs bind:value={selectedVendor}>
        <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
          <TabsList>
            {#each vendorTabs as tab (tab.key)}
              <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
            {/each}
            <TabsTrigger value={OTHER_TAB}>{t('gameDetails.otherTab')}</TabsTrigger>
          </TabsList>

          <div class="flex flex-wrap items-center gap-2">
            <BatchDownloadProgressBar
              value={downloadValue}
              max={downloadCount}
              active={showProgress}
            />
            <Tooltip>
              <TooltipTrigger>
                <Button
                  variant="default"
                  size="sm"
                  disabled={busy || renodx.busy || nothingToUpdate}
                  aria-busy={showProgress}
                  onclick={handleUpdateAll}
                >
                  {#if showProgress}
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
          <TabsContent value={tab.key} class="grid gap-3">
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
          </TabsContent>
        {/each}

        <TabsContent value={OTHER_TAB} class="grid gap-3">
          <RenoDxCard {gameId} {busy} store={renodx} {onOpenRenoDxSettings} />
        </TabsContent>
      </Tabs>
    {/if}
  </section>
</ScrollArea>
