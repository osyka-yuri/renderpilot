<script lang="ts">
  import {
    AddonAttribution,
    AddonInstallableView,
    AddonStateMessage,
    createConfidenceLabelKeys,
    createInstallableLabels,
  } from '@entities/addon';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import { Badge, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import type { LumaStore } from '../model/create-luma-store.svelte';
  import { LUMA_ATTRIBUTION } from '../model/attribution';
  import type { LumaEngine } from '../model/types';
  import { riskMessage } from '../model/luma-presenters';
  import LumaDgVoodooCallout from './LumaDgVoodooCallout.svelte';
  import LumaFeatures from './LumaFeatures.svelte';
  import LumaGuidanceCallouts from './LumaGuidanceCallouts.svelte';
  import LumaLaunchArgsCallout from './LumaLaunchArgsCallout.svelte';
  import LumaVcredistCallout from './LumaVcredistCallout.svelte';

  type Props = {
    gameId: string;
    store: LumaStore;
    busy: boolean;
    launcher: string;
  };

  const { gameId, store, busy, launcher }: Props = $props();

  const LUMA_INSTALLABLE_LABELS = createInstallableLabels('gameDetails.luma');
  const CONFIDENCE_LABEL_KEY = createConfidenceLabelKeys('gameDetails.luma');

  const GENERIC_ENGINE_LABEL: Record<LumaEngine, MessageKeyWithoutParams> = {
    unreal: 'gameDetails.luma.generic.engineUnreal',
    unity: 'gameDetails.luma.generic.engineUnity',
  };

  const riskText = $derived.by((): string => {
    const risk = store.risk;

    if (!risk) {
      return '';
    }

    return riskMessage(risk);
  });

  const genericEngine = $derived(store.profile?.scope === 'engine' ? store.profile.engine : null);
  const genericEngineLabel = $derived(
    genericEngine ? t(GENERIC_ENGINE_LABEL[genericEngine]) : null,
  );

  function onInstall(gid: string, force: boolean): void {
    void store.install(gid, force);
  }
</script>

<AddonInstallableView
  {gameId}
  {store}
  {busy}
  labels={LUMA_INSTALLABLE_LABELS}
  confidenceLabelKey={CONFIDENCE_LABEL_KEY}
  {onInstall}
  {riskText}
>
  {#snippet confidenceTrailing()}
    {#if genericEngineLabel}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <Badge {...props} variant="outline">
              {genericEngineLabel}
              <CircleHelpIcon class="size-3" aria-hidden="true" />
            </Badge>
          {/snippet}
        </TooltipTrigger>

        <TooltipContent>{t('gameDetails.luma.generic.profileTooltip')}</TooltipContent>
      </Tooltip>
    {/if}
  {/snippet}

  {#snippet preConflictWarnings()}
    <LumaFeatures features={store.features} />
    {#if store.installTorn}
      <AddonStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.luma.installTornWarning')}
      />
    {/if}
  {/snippet}

  {#snippet midCallouts()}
    {#if store.vcredistPresent === false}
      <LumaVcredistCallout installerUrl={store.vcredistInstallerUrl} />
    {/if}
    <LumaDgVoodooCallout requirement={store.externalRequirement} />
    <LumaGuidanceCallouts guidance={store.guidance} />
    <LumaLaunchArgsCallout launchArgs={store.launchArgs} {launcher} />
  {/snippet}

  {#snippet actionRowLeading()}
    <AddonAttribution {...LUMA_ATTRIBUTION} />
  {/snippet}
</AddonInstallableView>
