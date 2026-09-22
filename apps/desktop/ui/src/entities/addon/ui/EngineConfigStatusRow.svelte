<script lang="ts">
  import type { Snippet } from 'svelte';

  import FileCode2Icon from '@lucide/svelte/icons/file-code-2';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import WrenchIcon from '@lucide/svelte/icons/wrench';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemMedia,
    ItemTitle,
  } from '@shared/ui';

  import { shouldShowEngineConfigRow } from '../model/engine-config-guidance';
  import type { EngineConfigAvailability } from '../model/types';

  type Props = {
    availability?: EngineConfigAvailability;
    busy?: boolean;
    refreshing?: boolean;
    onApply?: () => void;
    onRefresh?: () => void;
    manualGuidance?: Snippet;
  };

  const {
    availability,
    busy = false,
    refreshing = false,
    onApply,
    onRefresh,
    manualGuidance,
  }: Props = $props();
  let manualDialogOpen = $state(false);

  const description = $derived.by((): string | null => {
    switch (availability?.status) {
      case 'manual_only':
        return t('gameDetails.engineConfig.status.manualOnly');
      case 'ready':
        return t('gameDetails.engineConfig.status.ready');
      case 'configured':
        return t('gameDetails.engineConfig.status.configured');
      case 'needs_repair':
        return t('gameDetails.engineConfig.status.needsRepair');
      case 'pending_first_launch':
        return t('gameDetails.engineConfig.status.pendingFirstLaunch');
      case 'conflict':
        return t('gameDetails.engineConfig.status.conflict');
      case 'recovery_required':
        return t('gameDetails.engineConfig.status.recoveryRequired');
      default:
        return null;
    }
  });
  const canApply = $derived(
    (availability?.status === 'ready' ||
      availability?.status === 'needs_repair' ||
      availability?.status === 'recovery_required') &&
      availability.can_apply &&
      Boolean(onApply),
  );
  const canRefresh = $derived(
    availability?.status === 'pending_first_launch' && Boolean(onRefresh),
  );
  const canConfigureManually = $derived(
    manualGuidance !== undefined &&
      (availability?.status === 'manual_only' ||
        availability?.status === 'pending_first_launch' ||
        availability?.status === 'conflict'),
  );
  const isWarning = $derived(availability?.status === 'conflict');
  const actionBusy = $derived(busy || refreshing);

  function openManualDialog(): void {
    if (canConfigureManually) {
      manualDialogOpen = true;
    }
  }
</script>

{#if shouldShowEngineConfigRow(availability?.status)}
  <Item size="sm" class={isWarning ? 'border-warning/40 bg-warning/10' : undefined}>
    <ItemMedia>
      <FileCode2Icon
        class={`size-4 ${isWarning ? 'text-warning' : 'text-muted-foreground'}`}
        aria-hidden="true"
      />
    </ItemMedia>

    <ItemContent class="min-w-0">
      <ItemTitle>{t('gameDetails.engineConfig.title')}</ItemTitle>
      {#if description}
        <ItemDescription>{description}</ItemDescription>
      {/if}
    </ItemContent>

    {#if canApply || canRefresh || canConfigureManually}
      <ItemActions class="self-start">
        {#if canRefresh}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={actionBusy}
            aria-busy={refreshing}
            onclick={onRefresh}
          >
            <RefreshCwIcon class={cn('size-4', refreshing && 'animate-spin')} aria-hidden="true" />
            {t('gameDetails.engineConfig.action.check')}
          </Button>
          <span class="sr-only" role="status">
            {#if refreshing}
              {t('gameDetails.engineConfig.action.checking')}
            {/if}
          </span>
        {/if}
        {#if canConfigureManually}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={actionBusy}
            onclick={openManualDialog}
          >
            <WrenchIcon class="size-4" aria-hidden="true" />
            {t('gameDetails.engineConfig.action.manual')}
          </Button>
        {/if}
        {#if canApply}
          <Button type="button" variant="outline" size="sm" disabled={actionBusy} onclick={onApply}>
            {t('common.apply')}
          </Button>
        {/if}
      </ItemActions>
    {/if}
  </Item>

  {#if canConfigureManually}
    <Dialog bind:open={manualDialogOpen}>
      <DialogContent
        class="max-h-[min(42rem,calc(100vh-2rem))] overflow-y-auto sm:max-w-2xl"
        closeLabel={t('common.close')}
      >
        <DialogHeader>
          <DialogTitle>{t('gameDetails.engineConfig.dialog.title')}</DialogTitle>
          {#if availability?.path}
            <DialogDescription class="font-mono text-xs break-all">
              {availability.path}
            </DialogDescription>
          {/if}
        </DialogHeader>
        <div class="min-w-0">{@render manualGuidance?.()}</div>
      </DialogContent>
    </Dialog>
  {/if}
{/if}
