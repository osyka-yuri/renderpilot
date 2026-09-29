<script lang="ts">
  import ArrowUpToLineIcon from '@lucide/svelte/icons/arrow-up-to-line';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import { t } from '@shared/i18n';
  import {
    Button,
    Progress,
    TabsList,
    TabsTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { ADDONS_TAB_VALUE, type VendorTab } from '../model/game-details-tabs';
  import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type {
    ExecutableLockReason,
    ProfileSelectionBlockReason,
  } from '../model/game-executable-lock';
  import GameExecutablePopover from './GameExecutablePopover.svelte';

  type Props = {
    title: string;
    vendorTabs: readonly VendorTab[];
    hasAddonsTab: boolean;
    gameId: string;
    exe: GameExecutableContext;
    lockReason: ExecutableLockReason | null;
    ownedBindingPath: string | null;
    profileSelectionBlockReason?: ProfileSelectionBlockReason | null;
    onMoveProfile: (path: string, selectAutomatically: boolean) => boolean | Promise<boolean>;
    showProgress: boolean;
    downloadCount: number;
    downloadValue: number;
    updatingAll: boolean;
    capturingUpdateAllSafety: boolean;
    planningUpdateAll: boolean;
    busy: boolean;
    addonsBusy: boolean;
    nothingToUpdate: boolean;
    totalUpdateCount: number;
    onUpdateAll: () => void | Promise<void>;
    onOpenOperations?: () => void;
    onPreloadOperations: () => void;
  };

  const {
    title,
    vendorTabs,
    hasAddonsTab,
    gameId,
    exe,
    lockReason,
    ownedBindingPath,
    profileSelectionBlockReason = null,
    onMoveProfile,
    showProgress,
    downloadCount,
    downloadValue,
    updatingAll,
    capturingUpdateAllSafety,
    planningUpdateAll,
    busy,
    addonsBusy,
    nothingToUpdate,
    totalUpdateCount,
    onUpdateAll,
    onOpenOperations,
    onPreloadOperations,
  }: Props = $props();

  const updateBusy = $derived(updatingAll || capturingUpdateAllSafety || planningUpdateAll);
  const updateDisabled = $derived(updateBusy || busy || addonsBusy || nothingToUpdate);
  const updateRunningLabel = $derived(t('gameDetails.updateAll.running'));
  const updateAccessibleName = $derived(
    updateBusy
      ? updateRunningLabel
      : nothingToUpdate
        ? t('gameDetails.updateAll.upToDate')
        : t('gameDetails.updateAll.actionCount', { count: totalUpdateCount }),
  );
  const updateVisibleLabel = $derived(
    updateBusy
      ? updateRunningLabel
      : nothingToUpdate
        ? t('gameDetails.updateAll.action')
        : t('gameDetails.updateAll.actionCount', { count: totalUpdateCount }),
  );
  const updateTooltip = $derived(
    updateBusy
      ? updateRunningLabel
      : nothingToUpdate
        ? t('gameDetails.updateAll.upToDate')
        : t('gameDetails.updateAll.tooltip', { count: totalUpdateCount }),
  );

  function guardUpdateClick(event: MouseEvent): void {
    if (updateDisabled) {
      event.preventDefault();
      return;
    }
    void onUpdateAll();
  }
</script>

<div class="flex min-w-0 shrink-0 items-center gap-3">
  {#if vendorTabs.length > 0 || hasAddonsTab}
    <div class="min-w-0 flex-1 overflow-x-auto">
      <TabsList aria-label={title} class="w-max">
        {#each vendorTabs as tab (tab.key)}
          <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
        {/each}
        {#if hasAddonsTab}
          <TabsTrigger value={ADDONS_TAB_VALUE}>{t('gameDetails.addonsTab')}</TabsTrigger>
        {/if}
      </TabsList>
    </div>
  {/if}

  <div class="ms-auto flex shrink-0 items-center gap-2">
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
            class="aria-disabled:pointer-events-auto"
            aria-disabled={updateDisabled}
            aria-busy={updateBusy}
            aria-label={updateAccessibleName}
            onclick={guardUpdateClick}
          >
            {#if updateBusy}
              <Loader2Icon class="animate-spin" aria-hidden="true" />
            {:else}
              <ArrowUpToLineIcon aria-hidden="true" />
            {/if}
            <span class="max-lg:hidden">{updateVisibleLabel}</span>
          </Button>
        {/snippet}
      </TooltipTrigger>
      <TooltipContent>
        {updateTooltip}
      </TooltipContent>
    </Tooltip>

    {#if onOpenOperations}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="secondary"
              size="sm"
              aria-label={t('operations.title')}
              onclick={onOpenOperations}
              onpointerenter={onPreloadOperations}
              onfocus={onPreloadOperations}
            >
              <HistoryIcon aria-hidden="true" />
              <span class="max-xl:hidden">{t('operations.title')}</span>
            </Button>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent>{t('operations.title')}</TooltipContent>
      </Tooltip>
    {/if}

    <GameExecutablePopover
      {gameId}
      {exe}
      {lockReason}
      {ownedBindingPath}
      {profileSelectionBlockReason}
      {onMoveProfile}
    />
  </div>
</div>
