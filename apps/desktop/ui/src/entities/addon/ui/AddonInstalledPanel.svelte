<script lang="ts">
  import type { Component, Snippet } from 'svelte';

  import { t } from '@shared/i18n';
  import { Badge, Button, ItemGroup, Spinner } from '@shared/ui';
  import CalendarIcon from '@lucide/svelte/icons/calendar';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import ClockIcon from '@lucide/svelte/icons/clock';
  import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
  import WrenchIcon from '@lucide/svelte/icons/wrench';

  import { formatDate, formatHttpDate, formatRelative } from '@shared/format';

  import type { AddonInstalledLabels } from '../model/presenters';
  import { actionDisabledMessage } from '../model/presenters';
  import type { AddonBadgeStatus } from '../model/badge-status';
  import type { AddonStoreView } from '../model/store-view';
  import AddonComponentRow from './AddonComponentRow.svelte';
  import AddonFieldLabel from './AddonFieldLabel.svelte';
  import AddonStateMessage from './AddonStateMessage.svelte';
  import AddonUninstallAction from './AddonUninstallAction.svelte';

  type PanelStore = Pick<
    AddonStoreView,
    | 'busy'
    | 'freshness'
    | 'addonDated'
    | 'installedAt'
    | 'lastCheckedAt'
    | 'requiresConfirmation'
    | 'hostFacts'
    | 'hostActions'
    | 'hostUpdate'
    | 'addonUpdate'
    | 'updateAvailable'
    | 'checkForUpdates'
    | 'update'
    | 'uninstall'
  >;

  type Props = {
    gameId: string;
    store: PanelStore;
    busy: boolean;
    labels: AddonInstalledLabels;
    StatusBadge: Component<{ status: AddonBadgeStatus }>;
    reshadeDescription: string;
    addonDescription: string;
    onRepair: () => void;
    topWarnings?: Snippet;
    afterDateCallouts?: Snippet;
    reshadeActions?: Snippet;
    extraComponentRows?: Snippet;
    afterComponents?: Snippet;
    downloadProgress?: Snippet;
  };

  const {
    gameId,
    store,
    busy,
    labels,
    StatusBadge,
    reshadeDescription,
    addonDescription,
    onRepair,
    topWarnings,
    afterDateCallouts,
    reshadeActions,
    extraComponentRows,
    afterComponents,
    downloadProgress,
  }: Props = $props();

  const addonDateLabel = $derived(formatHttpDate(store.addonDated));
  const installedLabel = $derived(store.installedAt ? formatDate(store.installedAt) : null);
  const checkedLabel = $derived(store.lastCheckedAt ? formatRelative(store.lastCheckedAt) : null);

  const isCheckingForUpdates = $derived(store.freshness === 'checking');
  const checkUpdatesDisabled = $derived(busy || isCheckingForUpdates);

  const updateAction = $derived(store.hostActions.update);
  const updateDisabledByHost = $derived(updateAction?.enabled === false);
  const updateDisabled = $derived(busy || updateDisabledByHost);
  const updateDisabledMessage = $derived(actionDisabledMessage(updateAction));

  const repairAction = $derived(store.hostActions.repair);
  const repairVisible = $derived(repairAction !== undefined);
  const repairDisabledByHost = $derived(repairAction?.enabled !== true);
  const repairDisabled = $derived(busy || repairDisabledByHost);
  const repairDisabledMessage = $derived(actionDisabledMessage(repairAction));

  const primaryHostDisabledMessage = $derived(updateDisabledMessage ?? repairDisabledMessage);

  const showFullAddonWarning = $derived(
    store.requiresConfirmation &&
      (store.hostFacts.addon_support === 'full' || updateAction !== undefined),
  );

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

    void store.update(gameId);
  }

  function handleUninstall(): Promise<boolean | undefined> | false {
    if (busy) {
      return false;
    }

    return store.uninstall(gameId);
  }
</script>

<div class="flex w-full flex-col gap-4">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
    <AddonFieldLabel label={t(labels.statusLabel)} class="flex-nowrap gap-1.5">
      <Badge variant="secondary">{t(labels.statusInstalled)}</Badge>
    </AddonFieldLabel>

    <AddonFieldLabel label={t(labels.freshnessLabel)} class="flex-nowrap gap-1.5">
      <StatusBadge status={store.freshness} />
    </AddonFieldLabel>
  </div>

  <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
    {#if addonDateLabel}
      <span class="text-foreground/80">
        <CalendarIcon class="mr-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
        {t(labels.addonDated, { date: addonDateLabel })}
      </span>
    {/if}

    {#if installedLabel}
      <span>{t(labels.installedOn, { date: installedLabel })}</span>
    {/if}

    <span>
      <ClockIcon class="mr-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
      {checkedLabel ? t(labels.lastChecked, { time: checkedLabel }) : t(labels.lastCheckedNever)}
    </span>
  </div>

  {@render topWarnings?.()}

  {#if showFullAddonWarning}
    <AddonStateMessage tone="warning" icon="warning" message={t(labels.fullAddonWarning)} />
  {/if}

  {@render afterDateCallouts?.()}

  <ItemGroup class="rounded-md border bg-muted/30">
    <AddonComponentRow
      icon="reshade"
      title={t(labels.componentReshade)}
      description={reshadeDescription}
      status={store.hostUpdate}
      {StatusBadge}
    >
      {#snippet actions()}
        {@render reshadeActions?.()}
      {/snippet}
    </AddonComponentRow>

    <AddonComponentRow
      icon="addon"
      title={t(labels.componentAddon)}
      description={addonDescription}
      status={store.addonUpdate}
      {StatusBadge}
    />

    {@render extraComponentRows?.()}
  </ItemGroup>

  {@render afterComponents?.()}

  {#if primaryHostDisabledMessage}
    <AddonStateMessage tone="warning" icon="warning" message={primaryHostDisabledMessage} />
  {/if}

  <div class="flex flex-wrap items-center justify-end gap-2 px-1">
    {#if downloadProgress}
      <div class="mr-auto flex-1">
        {@render downloadProgress()}
      </div>
    {/if}

    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={checkUpdatesDisabled}
      onclick={handleCheckForUpdates}
    >
      {#if isCheckingForUpdates}
        <Spinner class="size-4" />
        {t(labels.checking)}
      {:else}
        <RotateCwIcon class="size-4" aria-hidden="true" />
        {t(labels.actionCheckUpdates)}
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
          {t(labels.updating)}
        {:else}
          <CircleArrowUpIcon class="size-4" aria-hidden="true" />
          {t(labels.actionUpdate)}
        {/if}
      </Button>
    {/if}

    {#if repairVisible}
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={repairDisabled}
        onclick={onRepair}
      >
        <WrenchIcon class="size-4" aria-hidden="true" />
        {t(labels.actionRepair)}
      </Button>
    {/if}

    <AddonUninstallAction
      {busy}
      actionKey={labels.actionUninstall}
      confirmTitleKey={labels.uninstallConfirmTitle}
      confirmBodyKey={labels.uninstallConfirmBody}
      confirmActionKey={labels.uninstallConfirmAction}
      onConfirm={handleUninstall}
    />
  </div>
</div>
