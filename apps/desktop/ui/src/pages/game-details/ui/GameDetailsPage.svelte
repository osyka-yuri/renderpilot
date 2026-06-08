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
  } from '@shared/ui';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import { t } from '@shared/i18n';
  import type { SettingFamily } from '@features/nvapi-settings';
  import type {
    SwapHandler,
    RollbackHandler,
    BulkSwapHandler,
    BulkRollbackHandler,
  } from '../model/create-game-details-page-model';
  import { createNvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
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
  }: Props = $props();

  const tabs = $derived(createVendorTabs(details));

  const hasNvidiaTab = $derived(tabs.some((tab) => tab.key === 'nvidia'));
  const gameId = $derived(details?.game.identity.id ?? null);

  // The active vendor tab is user-controlled state, not derived: a post-swap
  // details reload re-derives `tabs`, and a hardcoded `value={tabs[0].key}`
  // would snap the user back to the first vendor (NVIDIA) every time. Reconcile
  // only when the set of available tabs changes — keep the current selection if
  // that vendor still has components, otherwise fall back to the first tab.
  let selectedVendor = $state('');
  $effect(() => {
    const keys = tabs.map((tab) => tab.key as string);
    untrack(() => {
      if (keys.length === 0) {
        selectedVendor = '';
      } else if (!keys.includes(selectedVendor)) {
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
    if (!details) return null;
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
    {:else if tabs.length === 0}
      <Card>
        <CardContent>
          <CardTitle>{t('gameDetails.noComponents.title')}</CardTitle>
          <CardDescription>
            {t('gameDetails.noComponents.description')}
          </CardDescription>
        </CardContent>
      </Card>
    {:else}
      <Tabs bind:value={selectedVendor}>
        <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
          <TabsList>
            {#each tabs as tab (tab.key)}
              <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
            {/each}
          </TabsList>

          {#if onOpenOperations}
            <Button variant="secondary" size="sm" onclick={onOpenOperations}>
              <HistoryIcon class="mr-2 size-4" />
              {t('operations.title')}
            </Button>
          {/if}
        </div>

        {#each tabs as tab (tab.key)}
          <TabsContent value={tab.key} class="grid gap-3">
            {#if tab.key === 'nvidia'}
              {#if gameId && nvidia.nvapiAvailable}
                <NvidiaProfileCard {gameId} nvapi={nvidia} />
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
      </Tabs>
    {/if}
  </section>
</ScrollArea>
