<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { t } from '@shared/i18n';
  import { Badge, Button, ItemGroup, Spinner } from '@shared/ui';
  import CalendarIcon from '@lucide/svelte/icons/calendar';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import ClockIcon from '@lucide/svelte/icons/clock';
  import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import WrenchIcon from '@lucide/svelte/icons/wrench';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import type { ReshadeChannel } from '../model/types';
  import { formatDate, formatHttpDate, formatRelative } from '../model/format';
  import {
    CHANNEL_LABEL,
    VULKAN_DIAGNOSTIC_LABEL,
    actionDisabledMessage,
    getAddonDescriptionKey,
    getReshadeDescription,
    type ReshadeDescriptionPart,
  } from '../model/reshade-presenters';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxComponentRow from './RenoDxComponentRow.svelte';
  import RenoDxFieldLabel from './RenoDxFieldLabel.svelte';
  import RenoDxStateMessage from './RenoDxStateMessage.svelte';
  import RenoDxStatusBadge from './RenoDxStatusBadge.svelte';
  import RenoDxUninstallAction from './RenoDxUninstallAction.svelte';
  import RenoDxUpdateConfirmDialog from './RenoDxUpdateConfirmDialog.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Combined busy flag: page-global or store mutation in flight. */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const downloadIds = $derived.by(() => [gameId]);

  const addonDateLabel = $derived(formatHttpDate(store.addonDated));
  const installedLabel = $derived(store.installedAt ? formatDate(store.installedAt) : null);
  const checkedLabel = $derived(store.lastCheckedAt ? formatRelative(store.lastCheckedAt) : null);

  const isCheckingForUpdates = $derived(store.freshness === 'checking');
  const checkUpdatesDisabled = $derived(busy || isCheckingForUpdates);

  const updateAction = $derived(store.hostActions.update);
  const updateDisabledByHost = $derived(updateAction?.enabled === false);
  const updateDisabled = $derived(busy || updateDisabledByHost);
  const updateRequiresConfirmation = $derived(updateAction?.requires_confirmation === true);
  const updateDisabledMessage = $derived(actionDisabledMessage(updateAction));

  let confirmUpdateDialogOpen = $state(false);

  const repairAction = $derived(store.hostActions.repair);
  const repairVisible = $derived(repairAction !== undefined);
  const repairDisabledByHost = $derived(repairAction?.enabled !== true);
  const repairDisabled = $derived(busy || repairDisabledByHost);
  const repairDisabledMessage = $derived(actionDisabledMessage(repairAction));

  const primaryHostDisabledMessage = $derived(updateDisabledMessage ?? repairDisabledMessage);

  const hasVulkanDiagnostics = $derived(store.vulkanUpdateDiagnostics.length > 0);

  const showFullAddonWarning = $derived(
    store.requiresConfirmation &&
      (store.hostFacts.addon_support === 'full' || updateAction !== undefined),
  );

  const reshadeChannelLabel = $derived(
    store.reshadeChannel ? t(CHANNEL_LABEL[store.reshadeChannel]) : null,
  );

  const installedChannelValue = $derived(store.reshadeChannel ?? store.selectedReshadeChannel);

  const showChannelControl = $derived(
    store.state?.status === 'installed' && store.state.host_kind === 'proxy',
  );

  const channelSwitchEnabled = $derived(store.hostActions.switch_channel?.enabled === true);
  const channelControlBusy = $derived(busy || !channelSwitchEnabled);

  const reshadeDescription = $derived.by((): string => {
    const description = getReshadeDescription({
      detection: store.hostDetection,
      facts: store.hostFacts,
    });

    if (description.kind === 'conflict') {
      return t(description.key);
    }

    const parts = description.parts.map(renderReshadeDescriptionPart);

    return parts.length > 0 ? parts.join(' · ') : t(description.fallbackKey);
  });

  /*
   * Read from install state, not from the latest update report. This keeps the
   * description correct on initial load, while probing, and after probe errors.
   */
  const addonDescription = $derived(
    t(getAddonDescriptionKey(store.renodxAddon, store.addonTracked)),
  );

  const showDlssFixRow = $derived(store.dlssFixInstalled || store.dlssFixAvailable);
  const dlssFixDescription = $derived(
    store.dlssFixInstalled
      ? t('gameDetails.renodx.component.dlssFixDesc')
      : t('gameDetails.renodx.component.dlssFixOffer'),
  );
  const dlssFixStatus = $derived(store.dlssFixInstalled ? store.dlssFixUpdate : undefined);

  function renderReshadeDescriptionPart(part: ReshadeDescriptionPart): string {
    return part.kind === 'version' ? t(part.key, { version: part.version }) : t(part.key);
  }

  function handleCheckForUpdates(): void {
    if (checkUpdatesDisabled) {
      return;
    }

    void store.checkForUpdates(gameId);
  }

  function handleUpdate(): void {
    if (updateDisabled || !store.updateAvailable) {
      return;
    }

    if (updateRequiresConfirmation) {
      confirmUpdateDialogOpen = true;
      return;
    }

    void store.update(gameId);
  }

  function handleSwitchChannel(channel: ReshadeChannel): void {
    if (busy || !channelSwitchEnabled || channel === installedChannelValue) {
      return;
    }

    void store.switchChannel(gameId, channel);
  }

  function handleInstallDlssFix(): void {
    if (busy || !store.dlssFixAvailable) {
      return;
    }

    void store.installDlssFix(gameId);
  }

  function handleUninstallDlssFix(): void {
    if (busy || !store.dlssFixInstalled) {
      return;
    }

    void store.uninstallDlssFix(gameId);
  }

  function handleRepair(): void {
    if (repairDisabled) {
      return;
    }

    void store.install(gameId, store.selectedReshadeChannel, false);
  }

  function handleUninstall(): void {
    if (busy) {
      return;
    }

    void store.uninstall(gameId);
  }
</script>

<div class="flex w-full flex-col gap-4">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
    <RenoDxFieldLabel label={t('gameDetails.renodx.status.label')} class="flex-nowrap gap-1.5">
      <Badge variant="secondary">{t('gameDetails.renodx.statusInstalled')}</Badge>
    </RenoDxFieldLabel>

    <RenoDxFieldLabel label={t('gameDetails.renodx.fresh.label')} class="flex-nowrap gap-1.5">
      <RenoDxStatusBadge status={store.freshness} />
    </RenoDxFieldLabel>
  </div>

  <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
    {#if addonDateLabel}
      <span class="text-foreground/80">
        <CalendarIcon class="mr-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
        {t('gameDetails.renodx.addonDated', { date: addonDateLabel })}
      </span>
    {/if}

    {#if installedLabel}
      <span>{t('gameDetails.renodx.installedOn', { date: installedLabel })}</span>
    {/if}

    <span>
      <ClockIcon class="mr-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
      {checkedLabel
        ? t('gameDetails.renodx.lastChecked', { time: checkedLabel })
        : t('gameDetails.renodx.lastCheckedNever')}
    </span>
  </div>

  {#if showFullAddonWarning}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.fullAddonWarning')}
    />
  {/if}

  <ItemGroup class="rounded-md border bg-muted/30">
    <RenoDxComponentRow
      icon="reshade"
      title={t('gameDetails.renodx.component.reshade')}
      description={reshadeDescription}
      status={store.hostUpdate}
    >
      {#snippet actions()}
        {#if showChannelControl}
          <RenoDxChannelControl
            value={installedChannelValue}
            stableSupported={store.reshadeStableSupported}
            busy={channelControlBusy}
            ariaLabel={t('gameDetails.renodx.channel.label')}
            onChange={handleSwitchChannel}
          />
        {:else if reshadeChannelLabel}
          <Badge variant="outline">{reshadeChannelLabel}</Badge>
        {/if}
      {/snippet}
    </RenoDxComponentRow>

    <RenoDxComponentRow
      icon="addon"
      title={t('gameDetails.renodx.component.addon')}
      description={addonDescription}
      status={store.addonUpdate}
    />

    {#if showDlssFixRow}
      <RenoDxComponentRow
        icon="dlssfix"
        title={t('gameDetails.renodx.component.dlssFix')}
        description={dlssFixDescription}
        hint={t('gameDetails.renodx.component.dlssFixHint')}
        status={dlssFixStatus}
      >
        {#snippet actions()}
          {#if store.dlssFixInstalled}
            <Button
              type="button"
              variant="destructive"
              size="sm"
              disabled={busy}
              onclick={handleUninstallDlssFix}
            >
              <Trash2Icon class="size-4" aria-hidden="true" />
              {t('gameDetails.renodx.actionRemoveDlssFix')}
            </Button>
          {:else}
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onclick={handleInstallDlssFix}
            >
              {t('gameDetails.renodx.actionInstallDlssFix')}
            </Button>
          {/if}
        {/snippet}
      </RenoDxComponentRow>
    {/if}
  </ItemGroup>

  {#if hasVulkanDiagnostics}
    <ul class="list-inside list-disc text-sm text-muted-foreground">
      {#each store.vulkanUpdateDiagnostics as reason (reason)}
        <li>{t(VULKAN_DIAGNOSTIC_LABEL[reason])}</li>
      {/each}
    </ul>
  {/if}

  {#if primaryHostDisabledMessage}
    <RenoDxStateMessage tone="warning" icon="warning" message={primaryHostDisabledMessage} />
  {/if}

  <div class="flex flex-wrap items-center justify-end gap-2 px-1">
    <DownloadProgressBar ids={downloadIds} active={store.busy} />

    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={checkUpdatesDisabled}
      onclick={handleCheckForUpdates}
    >
      {#if isCheckingForUpdates}
        <Spinner class="size-4" />
        {t('gameDetails.renodx.fresh.checking')}
      {:else}
        <RotateCwIcon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.actionCheckUpdates')}
      {/if}
    </Button>

    {#if store.updateAvailable}
      <Button
        type="button"
        variant="default"
        size="sm"
        disabled={updateDisabled}
        onclick={handleUpdate}
      >
        {#if store.busy}
          <Spinner class="size-4" />
          {t('gameDetails.renodx.updating')}
        {:else}
          <CircleArrowUpIcon class="size-4" aria-hidden="true" />
          {t('gameDetails.renodx.actionUpdate')}
        {/if}
      </Button>
    {/if}

    {#if repairVisible}
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={repairDisabled}
        onclick={handleRepair}
      >
        <WrenchIcon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.actionRepair')}
      </Button>
    {/if}

    <RenoDxUninstallAction {busy} onConfirm={handleUninstall} />
  </div>
</div>
<RenoDxUpdateConfirmDialog
  bind:open={confirmUpdateDialogOpen}
  {busy}
  onConfirm={() => store.update(gameId)}
/>
