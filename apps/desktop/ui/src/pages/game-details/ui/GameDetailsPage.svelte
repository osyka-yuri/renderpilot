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
  import type { SwapHandler, RollbackHandler } from '../model/create-game-details-page-model';
  import { createNvapiContext } from '../model/create-nvapi-context.svelte';
  import NvidiaProfileCard from './NvidiaProfileCard.svelte';
  import DlssSrComponentCard from './DlssSrComponentCard.svelte';
  import StreamlineComponentCard from './StreamlineComponentCard.svelte';
  import VendorComponentCard from './VendorComponentCard.svelte';

  const DLSS_SR_TECHNOLOGY = 'dlss_super_resolution';
  const NVIDIA_STREAMLINE_TECHNOLOGY = 'nvidia_streamline';

  type Props = {
    details?: GameDetails | null;
    busy?: boolean;
    /**
     * Whether the process is running elevated; controls NVAPI write
     * affordances (preset Select / revert buttons) inside DLSS cards.
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
   * Fingerprint of the currently installed DLSS SR DLL. Changes when the user
   * swaps the DLL (the new file has a different sha256 / version), which we
   * read inside the NVAPI reload effect so the DLL info badge and the
   * supported-preset list stay in sync without requiring a page revisit.
   */
  const dlssSrFingerprint = $derived.by(() => {
    if (!details) return null;
    const sr = details.components.find((c) => c.technology === DLSS_SR_TECHNOLOGY);
    const file = sr?.files[0];
    return file?.sha256 ?? file?.version ?? null;
  });

  // ── NVAPI shared context, owned by the page ──────────────────────
  const nvapi = createNvapiContext({ isElevated: () => isElevated });

  $effect(() => {
    // Reactive reads inside the effect determine when it re-runs:
    //   - gameId / hasNvidiaTab: standard load/teardown
    //   - dlssSrFingerprint:    re-load after DLL swap so the badge updates
    void dlssSrFingerprint;
    if (hasNvidiaTab && gameId) {
      void nvapi.reload(gameId);
    } else {
      nvapi.clear();
    }
  });

  function getCandidateGroup(componentId: string): GameCandidateGroup | null {
    return details?.candidate_groups.find((g) => g.component_id === componentId) ?? null;
  }

  function isDlssSr(component: GameGraphicsComponent): boolean {
    return component.technology === DLSS_SR_TECHNOLOGY;
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
                <NvidiaProfileCard {gameId} {nvapi} />
              {/if}

              {@const nonStreamline = tab.components.filter((c) => !isStreamline(c))}
              {@const streamline = tab.components.filter(isStreamline)}

              {#each nonStreamline as component (component.id)}
                {@const group = getCandidateGroup(component.id)}
                {#if isDlssSr(component) && gameId}
                  <DlssSrComponentCard
                    {gameId}
                    {component}
                    {group}
                    {nvapi}
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
