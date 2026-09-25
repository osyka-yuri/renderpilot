<script lang="ts">
  import type {
    GameCandidateGroup,
    GameDetails,
    GameFileSafetyAssessment,
    GameLibraryComponent,
  } from '@entities/game';
  import { GameFileSafetyRow } from '@entities/game';
  import type { SettingFamily } from '@features/nvapi-settings';
  import { LumaCard } from '@features/luma';
  import { OptiScalerCard } from '@features/optiscaler';
  import { RenoDxCard } from '@features/renodx';
  import { TabsContent } from '@shared/ui';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { createGameAddonsContext } from '../model/create-game-addons-context.svelte';
  import type { NvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';
  import {
    ADDONS_TAB_VALUE,
    DLSS_FAMILY_CARDS,
    NVIDIA_STREAMLINE_TECHNOLOGY,
    type VendorTab,
  } from '../model/game-details-tabs';
  import type {
    BulkRollbackHandler,
    BulkSwapHandler,
    RollbackHandler,
    SwapHandler,
  } from '../model/create-game-details-page-model';
  import DlssComponentCard from './DlssComponentCard.svelte';
  import NvidiaProfileControl from './NvidiaProfileControl.svelte';
  import StreamlineComponentCard from './StreamlineComponentCard.svelte';
  import VendorComponentCard from './VendorComponentCard.svelte';

  type GameAddonsContext = ReturnType<typeof createGameAddonsContext>;

  type Props = {
    details: GameDetails;
    gameId: string;
    profile: NvapiProfileContext;
    onOpenGameDetails: (gameId: string) => void | Promise<void>;
    onRecoveryDeleteComplete: () => void;
    vendorTabs: readonly VendorTab[];
    hasAddonsTab: boolean;
    assessment: GameFileSafetyAssessment | null;
    nvidia: NvidiaDriverContext;
    busy: boolean;
    exclusiveBusy: boolean;
    launcher: string;
    renodx: GameAddonsContext['stores']['renodx'];
    luma: GameAddonsContext['stores']['luma'];
    optiscaler: GameAddonsContext['stores']['optiscaler'];
    renodxEnabled: boolean;
    lumaEnabled: boolean;
    optiscalerEnabled: boolean;
    onSwap: SwapHandler;
    onRollback: RollbackHandler;
    onBulkSwap: BulkSwapHandler;
    onBulkRollback: BulkRollbackHandler;
    onOpenRenoDxSettings: () => void;
    onPreloadRenoDxSettings: () => void;
  };

  const {
    details,
    gameId,
    profile,
    onOpenGameDetails,
    onRecoveryDeleteComplete,
    vendorTabs,
    hasAddonsTab,
    assessment,
    nvidia,
    busy,
    exclusiveBusy,
    launcher,
    renodx,
    luma,
    optiscaler,
    renodxEnabled,
    lumaEnabled,
    optiscalerEnabled,
    onSwap,
    onRollback,
    onBulkSwap,
    onBulkRollback,
    onOpenRenoDxSettings,
    onPreloadRenoDxSettings,
  }: Props = $props();

  const hasNvidiaTab = $derived(vendorTabs.some((tab) => tab.key === 'nvidia'));
  const canCreateProfile = $derived(
    !nvidia.busy &&
      nvidia.nvapiAvailable &&
      details.components.some((component) => {
        const card = dlssFamilyCard(component);
        return card !== null && nvidia.settingsForFamily(card.family).length > 0;
      }),
  );

  function getCandidateGroup(componentId: string): GameCandidateGroup | null {
    return details.candidate_groups.find((group) => group.component_id === componentId) ?? null;
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

<div class="grid min-w-0 gap-4 p-1">
  {#if !hasNvidiaTab && details.game.platform === 'Windows'}
    <NvidiaProfileControl
      {gameId}
      mode="recovery"
      {profile}
      {onOpenGameDetails}
      {onRecoveryDeleteComplete}
      canCreate={false}
    />
  {/if}

  <GameFileSafetyRow {assessment} />

  {#each vendorTabs as tab (tab.key)}
    <TabsContent value={tab.key} class="mt-0 w-full min-w-0">
      <div class="grid min-w-0 gap-3">
        {#if tab.key === 'nvidia'}
          {@const nonStreamline = tab.components.filter((component) => !isStreamline(component))}
          {@const streamline = tab.components.filter(isStreamline)}

          {#if details.game.platform === 'Windows'}
            <NvidiaProfileControl
              {gameId}
              mode="settings"
              {profile}
              {onOpenGameDetails}
              {onRecoveryDeleteComplete}
              canCreate={canCreateProfile}
            />
          {/if}

          {#each nonStreamline as component (component.id)}
            {@const group = getCandidateGroup(component.id)}
            {@const dlssCard = dlssFamilyCard(component)}
            {#if dlssCard}
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
              streamline.map(
                (component) => [component.id, getCandidateGroup(component.id)] as const,
              ),
            )}
            <StreamlineComponentCard
              components={streamline}
              {groupsById}
              coordinatedOptions={details.streamline_candidate_options ?? []}
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
    </TabsContent>
  {/each}

  {#if hasAddonsTab}
    <TabsContent value={ADDONS_TAB_VALUE} class="mt-0 min-w-0">
      <div class="grid grid-cols-1 gap-3">
        {#if renodxEnabled}
          <RenoDxCard
            {gameId}
            busy={exclusiveBusy}
            {launcher}
            store={renodx}
            {onOpenRenoDxSettings}
            {onPreloadRenoDxSettings}
          />
        {/if}
        {#if lumaEnabled}
          <LumaCard {gameId} busy={exclusiveBusy} {launcher} store={luma} />
        {/if}
        {#if optiscalerEnabled}
          <OptiScalerCard {gameId} busy={exclusiveBusy} store={optiscaler} />
        {/if}
      </div>
    </TabsContent>
  {/if}
</div>
