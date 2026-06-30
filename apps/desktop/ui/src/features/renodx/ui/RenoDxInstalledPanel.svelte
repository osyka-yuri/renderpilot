<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { t } from '@shared/i18n';
  import { Badge, Button, ItemGroup, Spinner } from '@shared/ui';
  import CalendarIcon from '@lucide/svelte/icons/calendar';
  import ClockIcon from '@lucide/svelte/icons/clock';
  import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { formatDate, formatHttpDate, formatRelative } from '../model/format';
  import {
    CHANNEL_LABEL,
    canSwitchReshadeChannel,
    getAddonDescriptionKey,
    getReshadeDescription,
    getReshadeSwitchTarget,
    isReshadeSwitchDisabled,
    type ReshadeDescriptionPart,
  } from '../model/reshade-presenters';
  import RenoDxStatusBadge from './RenoDxStatusBadge.svelte';
  import RenoDxComponentRow from './RenoDxComponentRow.svelte';
  import RenoDxFieldLabel from './RenoDxFieldLabel.svelte';
  import RenoDxUninstallAction from './RenoDxUninstallAction.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Combined busy flag (page-global or store mutation in flight). */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const BRAND_LABEL = 'RenoDX';

  // The concrete "what's installed" anchor: the add-on's upstream date when known,
  // otherwise the local install date. (A rolling-snapshot add-on has no version.)
  const addonDateLabel = $derived(formatHttpDate(store.addonDated));
  const installedLabel = $derived(store.installedAt ? formatDate(store.installedAt) : null);
  const checkedLabel = $derived(store.lastCheckedAt ? formatRelative(store.lastCheckedAt) : null);
  const probing = $derived(store.freshness === 'checking');
  const reshadeHost = $derived(store.reshadeHost);

  function renderReshadeDescriptionPart(part: ReshadeDescriptionPart): string {
    if (part.kind === 'version') {
      return t(part.key, { version: part.version });
    }
    return t(part.key);
  }

  const reshadeChannelLabel = $derived(
    store.reshadeChannel ? t(CHANNEL_LABEL[store.reshadeChannel]) : null,
  );
  const reshadeDescription = $derived.by((): string => {
    const description = getReshadeDescription({
      host: reshadeHost,
      action: store.reshadeHostAction,
      conflict: store.reshadeConflict,
      ownership: store.reshadeOwnership,
    });
    if (description.kind === 'conflict') {
      return t(description.key);
    }
    const parts = description.parts.map(renderReshadeDescriptionPart);
    return parts.length > 0 ? parts.join(' · ') : t(description.fallbackKey);
  });
  const switchTarget = $derived(getReshadeSwitchTarget(store.reshadeChannel));
  const canSwitchChannel = $derived(canSwitchReshadeChannel(store.reshadeOwnership, switchTarget));
  const switchDisabled = $derived(
    isReshadeSwitchDisabled({
      busy,
      target: switchTarget,
      stableSupported: store.reshadeStableSupported,
    }),
  );
  const switchLabel = $derived(
    switchTarget
      ? t('gameDetails.renodx.channel.switchTo', { channel: t(CHANNEL_LABEL[switchTarget]) })
      : '',
  );

  // "Installed from a file — not tracked" reads straight off the install state
  // (`addonTracked`), so it is correct on load, during the probe, and after a
  // probe failure — unlike inferring it from the update report's `addon`, which
  // is `null` in all three cases.
  const addonDescription = $derived(
    t(getAddonDescriptionKey(store.renodxAddon, store.addonTracked)),
  );

  function checkForUpdates(): void {
    void store.checkForUpdates(gameId);
  }
  function update(): void {
    void store.update(gameId);
  }
  function uninstall(): void {
    void store.uninstall(gameId);
  }
  function installDlssFix(): void {
    void store.installDlssFix(gameId);
  }
  function uninstallDlssFix(): void {
    void store.uninstallDlssFix(gameId);
  }
  function switchChannel(): void {
    if (switchTarget) {
      void store.switchChannel(gameId, switchTarget);
    }
  }
</script>

<div class="flex w-full flex-col gap-4">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
    <RenoDxFieldLabel label={BRAND_LABEL} wrap={false} gap={1.5}>
      <Badge variant="secondary">{t('gameDetails.renodx.statusInstalled')}</Badge>
    </RenoDxFieldLabel>
    <RenoDxFieldLabel label={t('gameDetails.renodx.fresh.label')} wrap={false} gap={1.5}>
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
      <span>
        {t('gameDetails.renodx.installedOn', { date: installedLabel })}
      </span>
    {/if}
    <span>
      <ClockIcon class="mr-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
      {checkedLabel
        ? t('gameDetails.renodx.lastChecked', { time: checkedLabel })
        : t('gameDetails.renodx.lastCheckedNever')}
    </span>
  </div>

  <ItemGroup class="rounded-md border bg-muted/30">
    <RenoDxComponentRow
      icon="reshade"
      title={t('gameDetails.renodx.component.reshade')}
      description={reshadeDescription}
      status={store.hostUpdate}
    >
      {#snippet actions()}
        {#if reshadeChannelLabel}
          <Badge variant="outline">{reshadeChannelLabel}</Badge>
        {/if}
        {#if canSwitchChannel}
          <Button
            variant="outline"
            size="sm"
            disabled={switchDisabled}
            title={switchTarget === 'stable' && !store.reshadeStableSupported
              ? t('gameDetails.renodx.channel.stableUnavailable')
              : switchLabel}
            onclick={switchChannel}
          >
            {switchLabel}
          </Button>
        {/if}
      {/snippet}
    </RenoDxComponentRow>
    <RenoDxComponentRow
      icon="addon"
      title={t('gameDetails.renodx.component.addon')}
      description={addonDescription}
      status={store.addonUpdate}
    />
    {#if store.dlssFixInstalled}
      <RenoDxComponentRow
        icon="dlssfix"
        title={t('gameDetails.renodx.component.dlssFix')}
        description={t('gameDetails.renodx.component.dlssFixDesc')}
        hint={t('gameDetails.renodx.component.dlssFixHint')}
        status={store.dlssFixUpdate}
      >
        {#snippet actions()}
          <Button variant="destructive" size="sm" disabled={busy} onclick={uninstallDlssFix}>
            <Trash2Icon class="size-4" aria-hidden="true" />
            {t('gameDetails.renodx.actionRemoveDlssFix')}
          </Button>
        {/snippet}
      </RenoDxComponentRow>
    {:else if store.dlssFixAvailable}
      <RenoDxComponentRow
        icon="dlssfix"
        title={t('gameDetails.renodx.component.dlssFix')}
        description={t('gameDetails.renodx.component.dlssFixOffer')}
        hint={t('gameDetails.renodx.component.dlssFixHint')}
      >
        {#snippet actions()}
          <Button variant="outline" size="sm" disabled={busy} onclick={installDlssFix}>
            {t('gameDetails.renodx.actionInstallDlssFix')}
          </Button>
        {/snippet}
      </RenoDxComponentRow>
    {/if}
  </ItemGroup>

  <div class="flex flex-wrap items-center gap-2">
    <DownloadProgressBar ids={[gameId]} active={store.busy} />
    <Button variant="outline" size="sm" disabled={busy || probing} onclick={checkForUpdates}>
      {#if probing}
        <Spinner class="size-4" />
        {t('gameDetails.renodx.fresh.checking')}
      {:else}
        <RotateCwIcon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.actionCheckUpdates')}
      {/if}
    </Button>
    {#if store.updateAvailable}
      <Button variant="default" size="sm" disabled={busy} onclick={update}>
        {#if store.busy}
          <Spinner class="size-4" />
          {t('gameDetails.renodx.updating')}
        {:else}
          <CircleArrowUpIcon class="size-4" aria-hidden="true" />
          {t('gameDetails.renodx.actionUpdate')}
        {/if}
      </Button>
    {/if}
    <RenoDxUninstallAction {busy} onConfirm={uninstall} />
  </div>
</div>
