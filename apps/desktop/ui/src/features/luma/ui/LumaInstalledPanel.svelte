<script lang="ts">
  import {
    AddonAttribution,
    AddonComponentRow,
    AddonInstalledPanel,
    AddonStateMessage,
    createInstalledLabels,
    isReshadeChannel,
  } from '@entities/addon';
  import { t } from '@shared/i18n';
  import { Badge } from '@shared/ui';

  import type { LumaStore } from '../model/create-luma-store.svelte';
  import { LUMA_ATTRIBUTION } from '../model/attribution';
  import { dgVoodooRequirement } from '../model/external-requirements';
  import { payloadRepairAction as resolvePayloadRepairAction } from '../model/luma-store-helpers';
  import { describeReshadeHost } from '../model/luma-presenters';
  import LumaLaunchArgsCallout from './LumaLaunchArgsCallout.svelte';
  import LumaFeatures from './LumaFeatures.svelte';
  import LumaGuidanceCallouts from './LumaGuidanceCallouts.svelte';
  import LumaVcredistCallout from './LumaVcredistCallout.svelte';

  type Props = {
    gameId: string;
    store: LumaStore;
    busy: boolean;
    launcher: string;
  };

  const { gameId, store, busy, launcher }: Props = $props();

  // Payload force-full reconverge only when the install is torn. Host repair
  // still comes from `store.hostActions.repair` (backend) via the panel.
  // Never pass a permanent enabled repairAction — that showed "Repair" always.
  const payloadRepairAction = $derived(
    resolvePayloadRepairAction(store.installTorn, store.isInstallable),
  );

  const LUMA_INSTALLED_LABELS = createInstalledLabels('gameDetails.luma');

  const reshadeDescription = $derived(
    describeReshadeHost({
      detection: store.hostDetection,
      facts: store.hostFacts,
      actions: store.hostActions,
    }),
  );

  const addonDescription = $derived(t('gameDetails.luma.component.addonDesc'));
  const dgvoodooRequirement = $derived(dgVoodooRequirement(store.externalRequirement));
  // Keep the row when ownership status is known even if the installable
  // external_requirement disappeared (catalogue drift / non-installable outcome).
  const showDgVoodoo = $derived(dgvoodooRequirement != null || store.dgvoodooUpdate != null);
  const dgvoodooDescription = $derived(
    dgvoodooRequirement
      ? t('gameDetails.luma.component.dgvoodooDesc', { version: dgvoodooRequirement.version })
      : t('gameDetails.luma.component.dgvoodoo'),
  );

  const CHANNEL_LABEL = {
    stable: 'gameDetails.luma.channel.stable',
    nightly: 'gameDetails.luma.channel.nightly',
  } as const;

  const reshadeChannelLabel = $derived(
    isReshadeChannel(store.reshadeChannel) ? t(CHANNEL_LABEL[store.reshadeChannel]) : null,
  );

  function handleRepair(): void {
    void store.repair(gameId);
  }
</script>

<AddonInstalledPanel
  {gameId}
  {store}
  {busy}
  labels={LUMA_INSTALLED_LABELS}
  statusI18nPrefix="gameDetails.luma"
  {reshadeDescription}
  {addonDescription}
  onRepair={handleRepair}
  repairAction={payloadRepairAction}
>
  {#snippet topWarnings()}
    {#if store.installTorn}
      <AddonStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.luma.installTornWarningInstalled')}
      />
    {/if}
  {/snippet}

  {#snippet afterDateCallouts()}
    <LumaFeatures features={store.features} />
    <LumaGuidanceCallouts guidance={store.guidance} />
    {#if store.vcredistPresent === false}
      <LumaVcredistCallout installerUrl={store.vcredistInstallerUrl} />
    {/if}
    <LumaLaunchArgsCallout launchArgs={store.launchArgs} {launcher} />
  {/snippet}

  {#snippet reshadeActions()}
    {#if reshadeChannelLabel}
      <Badge variant="outline">{reshadeChannelLabel}</Badge>
    {/if}
  {/snippet}

  {#snippet extraComponentRows()}
    {#if showDgVoodoo}
      <AddonComponentRow
        icon="addon"
        title={t('gameDetails.luma.component.dgvoodoo')}
        description={dgvoodooDescription}
        status={store.dgvoodooUpdate}
        statusI18nPrefix="gameDetails.luma"
      />
    {/if}
  {/snippet}

  {#snippet actionRowLeading()}
    <AddonAttribution {...LUMA_ATTRIBUTION} />
  {/snippet}
</AddonInstalledPanel>
