<script lang="ts">
  import type { Component } from 'svelte';

  import {
    AddonComponentRow,
    AddonInstalledPanel,
    createInstalledLabels,
    type AddonBadgeStatus,
  } from '@entities/addon';
  import { t } from '@shared/i18n';
  import { Badge, Button } from '@shared/ui';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import { DownloadProgressBar } from '@entities/library';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import type { ReshadeChannel } from '../model/types';
  import {
    CHANNEL_LABEL,
    VULKAN_DIAGNOSTIC_LABEL,
    describeReshadeHost,
    getAddonDescriptionKey,
  } from '../model/reshade-presenters';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxStatusBadgeRaw from './RenoDxStatusBadge.svelte';

  const RenoDxStatusBadge: Component<{ status: AddonBadgeStatus }> = RenoDxStatusBadgeRaw;

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const RENODX_INSTALLED_LABELS = createInstalledLabels('gameDetails.renodx');

  const reshadeDescription = $derived(
    describeReshadeHost({
      detection: store.hostDetection,
      facts: store.hostFacts,
    }),
  );

  const addonDescription = $derived(
    t(getAddonDescriptionKey(store.renodxAddon, store.addonTracked)),
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

  const showDlssFixRow = $derived(store.dlssFixInstalled || store.dlssFixAvailable);
  const dlssFixDescription = $derived(
    store.dlssFixInstalled
      ? t('gameDetails.renodx.component.dlssFixDesc')
      : t('gameDetails.renodx.component.dlssFixOffer'),
  );
  const dlssFixStatus = $derived(store.dlssFixInstalled ? store.dlssFixUpdate : undefined);

  const progressIds = $derived([gameId]);

  function handleRepair(): void {
    void store.install(gameId, store.selectedReshadeChannel, false);
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
</script>

<AddonInstalledPanel
  {gameId}
  {store}
  {busy}
  labels={RENODX_INSTALLED_LABELS}
  StatusBadge={RenoDxStatusBadge}
  {reshadeDescription}
  {addonDescription}
  onRepair={handleRepair}
>
  {#snippet reshadeActions()}
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

  {#snippet extraComponentRows()}
    {#if showDlssFixRow}
      <AddonComponentRow
        icon="dlssfix"
        title={t('gameDetails.renodx.component.dlssFix')}
        description={dlssFixDescription}
        hint={t('gameDetails.renodx.component.dlssFixHint')}
        status={dlssFixStatus}
        StatusBadge={RenoDxStatusBadge}
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
      </AddonComponentRow>
    {/if}
  {/snippet}

  {#snippet afterComponents()}
    {#if store.vulkanUpdateDiagnostics.length > 0}
      <ul class="list-inside list-disc text-sm text-muted-foreground">
        {#each store.vulkanUpdateDiagnostics as reason (reason)}
          <li>{t(VULKAN_DIAGNOSTIC_LABEL[reason])}</li>
        {/each}
      </ul>
    {/if}
  {/snippet}

  {#snippet downloadProgress()}
    <DownloadProgressBar ids={progressIds} active={store.busy} />
  {/snippet}
</AddonInstalledPanel>
