<script lang="ts">
  import {
    AddonAttribution,
    AddonInstallableView,
    createConfidenceLabelKeys,
    createInstallableLabels,
  } from '@entities/addon';
  import { t, translateExternalMessage } from '@shared/i18n';
  import { Badge, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { RENODX_ATTRIBUTION } from '../model/attribution';
  import type { ReshadeChannel } from '@entities/addon';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const RENODX_INSTALLABLE_LABELS = createInstallableLabels('gameDetails.renodx');
  const CONFIDENCE_LABEL_KEY = createConfidenceLabelKeys('gameDetails.renodx');

  const genericProfileLabel = $derived.by((): string | null => {
    const profile = store.genericProfile;
    return profile
      ? translateExternalMessage({
          key: profile.message.id,
          fallback: profile.message.fallback_text,
        })
      : null;
  });

  function onInstall(gid: string): void {
    void store.install(gid, store.selectedReshadeChannel);
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
>
  {#snippet confidenceTrailing()}
    {#if genericProfileLabel}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <Badge {...props} variant="outline">
              {genericProfileLabel}
              <CircleHelpIcon class="size-3" aria-hidden="true" />
            </Badge>
          {/snippet}
        </TooltipTrigger>

        <TooltipContent>{t('gameDetails.renodx.generic.profileTooltip')}</TooltipContent>
      </Tooltip>
    {/if}
  {/snippet}

  {#snippet midCallouts()}
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

  {#snippet actionRowLeading()}
    <AddonAttribution {...RENODX_ATTRIBUTION} />
  {/snippet}
</AddonInstallableView>
