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
  import type { ExecutableLockReason } from '../model/game-executable-lock';
  import GameExecutablePopover from './GameExecutablePopover.svelte';

  type Props = {
    title: string;
    vendorTabs: readonly VendorTab[];
    hasAddonsTab: boolean;
    gameId: string;
    exe: GameExecutableContext;
    lockReason: ExecutableLockReason | null;
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
</script>

<div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
  {#if vendorTabs.length > 0 || hasAddonsTab}
    <TabsList aria-label={title}>
      {#each vendorTabs as tab (tab.key)}
        <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
      {/each}
      {#if hasAddonsTab}
        <TabsTrigger value={ADDONS_TAB_VALUE}>{t('gameDetails.otherTab')}</TabsTrigger>
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
            disabled={updateDisabled}
            aria-busy={updateBusy}
            onclick={onUpdateAll}
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

    <GameExecutablePopover {gameId} {exe} {lockReason} />
  </div>
</div>
