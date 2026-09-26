<script lang="ts">
  import {
    AddonAttribution,
    AddonComponentRow,
    AddonInstalledPanel,
    AddonStateMessage,
    EngineConfigStatusRow,
    createInstalledLabels,
  } from '@entities/addon';
  import { t } from '@shared/i18n';
  import { Badge, Button, LaunchArgumentsCallout } from '@shared/ui';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { RENODX_ATTRIBUTION } from '../model/attribution';
  import type { ReshadeChannel } from '@entities/addon';
  import {
    CHANNEL_LABEL,
    VULKAN_DIAGNOSTIC_LABEL,
    describeReshadeHost,
    getAddonDescriptionKey,
  } from '../model/reshade-presenters';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxGuidanceCallouts from './RenoDxGuidanceCallouts.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
    launcher: string;
  };

  const { gameId, store, busy, launcher }: Props = $props();

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

  const dlssFix = $derived(store.dlssFix);

  function handleRepair(): void {
    void store.install(gameId, store.selectedReshadeChannel);
  }

  function handleEngineConfigApply(): void {
    void store.applyEngineConfig(gameId);
  }

  function handleEngineConfigRefresh(): void {
    void store.refreshAvailability(gameId);
  }

  function handleSwitchChannel(channel: ReshadeChannel): void {
    if (busy || !channelSwitchEnabled || channel === installedChannelValue) {
      return;
    }

    void store.switchChannel(gameId, channel);
  }

  function handleDlssFixPrimaryAction(): void {
    if (busy || dlssFix.kind !== 'component' || !dlssFix.primaryAction) {
      return;
    }

    if (dlssFix.primaryAction.kind === 'install') {
      void store.installDlssFix(gameId);
      return;
    }

    void store.updateDlssFix(gameId);
  }

  function handleUninstallDlssFix(): void {
    if (busy || dlssFix.kind !== 'component' || !dlssFix.canRemove) {
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
  statusI18nPrefix="gameDetails.renodx"
  {reshadeDescription}
  {addonDescription}
  onRepair={handleRepair}
>
  {#snippet topWarnings()}
    {#if store.hasUnsupportedSettings}
      <AddonStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.renodx.unsupportedSettings')}
      />
    {/if}
    {#if store.installTorn}
      <AddonStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.renodx.installTornWarningInstalled')}
      />
    {/if}
  {/snippet}

  {#snippet afterDateCallouts()}
    <RenoDxGuidanceCallouts guidance={store.guidance} />
    <LaunchArgumentsCallout launch={store.launch} {launcher} />
  {/snippet}

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
    <EngineConfigStatusRow
      availability={store.engineConfig}
      {busy}
      refreshing={store.loading}
      onApply={handleEngineConfigApply}
      onRefresh={handleEngineConfigRefresh}
    >
      {#snippet manualGuidance()}
        <RenoDxGuidanceCallouts guidance={store.guidance} presentation="engine-ini-dialog" />
      {/snippet}
    </EngineConfigStatusRow>

    {#if dlssFix.kind === 'component'}
      {@const primaryAction = dlssFix.primaryAction}
      <AddonComponentRow
        icon="dlssfix"
        title={t('gameDetails.renodx.component.dlssFix')}
        description={t(dlssFix.descriptionKey)}
        hint={t('gameDetails.renodx.component.dlssFixHint')}
        status={dlssFix.status}
        statusI18nPrefix="gameDetails.renodx"
      >
        {#snippet actions()}
          {#if primaryAction}
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onclick={handleDlssFixPrimaryAction}
            >
              {t(primaryAction.labelKey)}
            </Button>
          {/if}
          {#if dlssFix.canRemove}
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

  {#snippet actionRowLeading()}
    <AddonAttribution {...RENODX_ATTRIBUTION} />
  {/snippet}
</AddonInstalledPanel>
