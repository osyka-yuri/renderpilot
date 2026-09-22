<script lang="ts">
  import {
    AddonAttribution,
    AddonInstallableView,
    AddonStateMessage,
    createConfidenceLabelKeys,
    createInstallableLabels,
  } from '@entities/addon';
  import { t, translateExternalMessage, type MessageKeyWithoutParams } from '@shared/i18n';
  import {
    Badge,
    LaunchArgumentsCallout,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import type { RenoDxEngine } from '../model/types';
  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { RENODX_ATTRIBUTION } from '../model/attribution';
  import type { ReshadeChannel } from '@entities/addon';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxGuidanceCallouts from './RenoDxGuidanceCallouts.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
    launcher: string;
  };

  const { gameId, store, busy, launcher }: Props = $props();

  const RENODX_INSTALLABLE_LABELS = createInstallableLabels('gameDetails.renodx');
  const CONFIDENCE_LABEL_KEY = createConfidenceLabelKeys('gameDetails.renodx');

  const GENERIC_PROFILE_LABEL_KEYS: Partial<Record<string, MessageKeyWithoutParams>> = {
    ue_extended: 'gameDetails.renodx.generic.profileUeExtended',
    unity: 'gameDetails.renodx.generic.profileUnity',
    unreal_legacy: 'gameDetails.renodx.generic.profileUnrealLegacy',
  };

  const GENERIC_ENGINE_FALLBACK: Record<RenoDxEngine, string> = {
    unity: 'Unity',
    unreal: 'Unreal Engine',
    unreal_extended: 'Unreal Engine',
  };

  const genericProfile = $derived(store.genericProfile);

  const genericProfileBadgeLabel = $derived.by((): string | null => {
    if (!genericProfile) {
      return null;
    }
    const key = genericProfile.profile_id
      ? GENERIC_PROFILE_LABEL_KEYS[genericProfile.profile_id]
      : undefined;
    if (key) {
      return t(key);
    }
    return GENERIC_ENGINE_FALLBACK[genericProfile.engine] ?? null;
  });

  const genericProfileTooltip = $derived.by((): string => {
    if (!genericProfile) {
      return t('gameDetails.renodx.generic.profileTooltip');
    }
    return (
      translateExternalMessage({
        key: genericProfile.message.id,
        fallback: genericProfile.message.fallback_text,
      }) || t('gameDetails.renodx.generic.profileTooltip')
    );
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
    {#if genericProfileBadgeLabel}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <Badge {...props} variant="outline">
              {genericProfileBadgeLabel}
              <CircleHelpIcon class="size-3" aria-hidden="true" />
            </Badge>
          {/snippet}
        </TooltipTrigger>

        <TooltipContent>{genericProfileTooltip}</TooltipContent>
      </Tooltip>
    {/if}
  {/snippet}

  {#snippet midCallouts()}
    <RenoDxGuidanceCallouts guidance={store.guidance} />
    <LaunchArgumentsCallout launch={store.launch} {launcher} />
    {#if store.installTorn}
      <AddonStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.renodx.installTornWarning')}
      />
    {/if}
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
