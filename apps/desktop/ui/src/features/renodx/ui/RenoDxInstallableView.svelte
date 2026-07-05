<script lang="ts">
  import { AddonInstallableView } from '@entities/addon';
  import type { AddonInstallableLabels } from '@entities/addon';
  import { t, type MessageKey } from '@shared/i18n';
  import { DownloadProgressBar } from '@entities/library';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import type { MatchConfidence, ReshadeChannel } from '../model/types';
  import { riskMessage } from '../model/reshade-presenters';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const RENODX_INSTALLABLE_LABELS = {
    installAction: 'gameDetails.renodx.actionInstall',
    installing: 'gameDetails.renodx.installing',
    confidenceLabel: 'gameDetails.renodx.confidenceLabel',
    hostCustomBuild: 'gameDetails.renodx.host.customBuild',
    hostConflictBlocksInstall: 'gameDetails.renodx.host.conflictBlocksInstall',
    fullAddonWarning: 'gameDetails.addon.fullAddonWarning',
    confirmTitle: 'gameDetails.renodx.confirmTitle',
    confirmBody: 'gameDetails.addon.confirmBody',
    confirmAccept: 'gameDetails.addon.confirmAccept',
  } as const satisfies AddonInstallableLabels;

  const CONFIDENCE_LABEL_KEY = {
    verified: 'gameDetails.renodx.confidenceVerified',
    experimental: 'gameDetails.renodx.confidenceExperimental',
    untested: 'gameDetails.renodx.confidenceUntested',
  } as const satisfies Record<MatchConfidence, MessageKey>;

  const riskText = $derived.by((): string => {
    const risk = store.risk;

    if (!risk) {
      return '';
    }

    return riskMessage(risk);
  });

  const progressIds = $derived([gameId]);

  function onInstall(gid: string, force: boolean): void {
    void store.install(gid, store.selectedReshadeChannel, force);
  }

  function setChannel(channel: ReshadeChannel): void {
    store.setSelectedReshadeChannel(channel);
  }
</script>

<AddonInstallableView
  {gameId}
  {store}
  {busy}
  labels={RENODX_INSTALLABLE_LABELS}
  confidenceLabelKey={CONFIDENCE_LABEL_KEY}
  {onInstall}
  {riskText}
>
  {#snippet preNotesCallouts()}
    {#if (store.outcome?.kind === 'installable' ? store.outcome.host_kind : null) === 'proxy'}
      <RenoDxChannelControl
        class="max-w-72"
        value={store.selectedReshadeChannel}
        stableSupported={store.reshadeStableSupported}
        {busy}
        label={t('gameDetails.renodx.channel.hostLabel')}
        onChange={setChannel}
      />
    {/if}
  {/snippet}

  {#snippet downloadProgress()}
    <DownloadProgressBar ids={progressIds} active={store.busy} />
  {/snippet}
</AddonInstallableView>
