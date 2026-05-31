<script lang="ts">
  import type { GameCandidateGroup, GameDetails, GameGraphicsComponent } from '@entities/game';
  import {
    libraryVendorOrder,
    libraryVendorKey,
    vendorLabelForLibraryVendorKey,
    type LibraryVendorKey,
  } from '@shared/graphics';
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
  } from '@shared/ui';
  import type { SettingFamily } from '@features/nvapi-settings';
  import type { SwapHandler, RollbackHandler } from '../model/create-game-details-page-model';
  import { createNvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import NvidiaProfileCard from './NvidiaProfileCard.svelte';
  import DlssComponentCard from './DlssComponentCard.svelte';
  import StreamlineComponentCard from './StreamlineComponentCard.svelte';
  import VendorComponentCard from './VendorComponentCard.svelte';

  const NVIDIA_STREAMLINE_TECHNOLOGY = 'nvidia_streamline';

  // Each DLSS DLL family is its own card (physical DLL swap + NVAPI driver
  // overrides), exactly like Super Resolution — keyed off the component's
  // technology. Streamline (sl.*.dll) is unrelated and keeps its own card.
  const DLSS_FAMILY_CARDS: Record<string, { family: SettingFamily; title: string }> = {
    dlss_super_resolution: { family: 'sr', title: 'NVIDIA DLSS Super Resolution' },
    dlss_frame_generation: { family: 'fg', title: 'NVIDIA DLSS Frame Generation' },
    dlss_ray_reconstruction: { family: 'rr', title: 'NVIDIA DLSS Ray Reconstruction' },
  };

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
  };

  const {
    details = null,
    busy = false,
    isElevated = true,
    onSwap = () => undefined,
    onRollback = () => undefined,
  }: Props = $props();

  type VendorTab = {
    key: LibraryVendorKey;
    label: string;
    components: GameGraphicsComponent[];
  };

  const tabs = $derived.by((): VendorTab[] => {
    if (!details) return [];

    const byVendor: Record<LibraryVendorKey, GameGraphicsComponent[]> = {
      nvidia: [],
      amd: [],
      intel: [],
      other: [],
    };

    for (const component of details.components) {
      const key = libraryVendorKey(component.technology);
      byVendor[key].push(component);
    }

    return libraryVendorOrder
      .map((key) => ({
        key,
        label: vendorLabelForLibraryVendorKey(key),
        components: byVendor[key],
      }))
      .filter((tab) => tab.components.length > 0);
  });

  const hasNvidiaTab = $derived(tabs.some((tab) => tab.key === 'nvidia'));
  const gameId = $derived(details?.game.identity.id ?? null);

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
          <CardTitle>No game selected</CardTitle>
          <CardDescription>
            Select a game card on the dashboard to open one coherent workspace for that
            installation.
          </CardDescription>
        </CardContent>
      </Card>
    {:else if tabs.length === 0}
      <Card>
        <CardContent>
          <CardTitle>No graphics components detected</CardTitle>
          <CardDescription>
            No graphics-related components were detected for this installation.
          </CardDescription>
        </CardContent>
      </Card>
    {:else}
      <Tabs value={tabs[0].key}>
        <TabsList>
          {#each tabs as tab (tab.key)}
            <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
          {/each}
        </TabsList>

        {#each tabs as tab (tab.key)}
          <TabsContent value={tab.key} class="grid gap-3">
            {#if tab.key === 'nvidia'}
              {#if gameId}
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
                  {onSwap}
                  {onRollback}
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
