<script lang="ts">
  import type { Snippet } from 'svelte';

  import CalendarIcon from '@lucide/svelte/icons/calendar';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import ClockIcon from '@lucide/svelte/icons/clock';
  import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
  import WrenchIcon from '@lucide/svelte/icons/wrench';
  import { formatLocalShortDate, formatRelativeTime, formatUtcShortDate } from '@shared/format';
  import { getLocale, t } from '@shared/i18n';
  import { Badge, Button, ItemGroup, Spinner } from '@shared/ui';

  import { isMutationSuccess } from '../model/busy-mutation';
  import type { AddonInstalledLabels } from '../model/presenters';
  import { actionDisabledMessage } from '../model/presenters';
  import type { AddonStoreView } from '../model/store-view';
  import type { ActionDescriptor } from '../model/types';
  import AddonComponentRow from './AddonComponentRow.svelte';
  import AddonFieldLabel from './AddonFieldLabel.svelte';
  import AddonStateMessage from './AddonStateMessage.svelte';
  import AddonToolStatusBadge from './AddonToolStatusBadge.svelte';
  import AddonUninstallAction from './AddonUninstallAction.svelte';
  import type { AddonToolI18nPrefix } from './types';

  type PanelStore = Pick<
    AddonStoreView,
    | 'busy'
    | 'freshness'
    | 'addonDated'
    | 'installedAt'
    | 'lastCheckedAt'
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
    statusI18nPrefix: AddonToolI18nPrefix;
    reshadeDescription: string;
    addonDescription: string;
    onRepair: () => void;
    /**
     * Tool-owned repair action when the host does not expose `hostActions.repair`
     * (e.g. Luma payload force-full reconverge). Host repair still wins when present.
     */
    repairAction?: ActionDescriptor;
    topWarnings?: Snippet;
    afterDateCallouts?: Snippet;
    reshadeActions?: Snippet;
    extraComponentRows?: Snippet;
    afterComponents?: Snippet;
    actionRowLeading?: Snippet;
  };

  const {
    gameId,
    store,
    busy,
    labels,
    statusI18nPrefix,
    reshadeDescription,
    addonDescription,
    onRepair,
    repairAction: toolRepairAction,
    topWarnings,
    afterDateCallouts,
    reshadeActions,
    extraComponentRows,
    afterComponents,
    actionRowLeading,
  }: Props = $props();

  const locale = $derived(getLocale());
  const addonDateLabel = $derived(
    store.addonDated === null ? null : formatUtcShortDate(store.addonDated, locale),
  );
  const installedLabel = $derived(
    store.installedAt === null ? null : formatLocalShortDate(store.installedAt, locale),
  );
  const checkedLabel = $derived(
    store.lastCheckedAt === null ? null : formatRelativeTime(store.lastCheckedAt, locale),
  );

  const isCheckingForUpdates = $derived(store.freshness === 'checking');
  const checkUpdatesDisabled = $derived(busy || isCheckingForUpdates);

  const updateAction = $derived(store.hostActions.update);
  const updateDisabledByHost = $derived(updateAction?.enabled === false);
  const updateDisabled = $derived(busy || updateDisabledByHost);
  const updateDisabledMessage = $derived(actionDisabledMessage(updateAction));

  // Host repair (under-min / repair host) takes precedence; tools may supply a
  // fallback descriptor (e.g. Luma payload force-full reconverge).
  const repairAction = $derived(store.hostActions.repair ?? toolRepairAction);
  const repairVisible = $derived(repairAction !== undefined);
  const repairDisabledByHost = $derived(repairAction?.enabled !== true);
  const repairDisabled = $derived(busy || repairDisabledByHost);
  const repairDisabledMessage = $derived(actionDisabledMessage(repairAction));

  const primaryHostDisabledMessage = $derived(updateDisabledMessage ?? repairDisabledMessage);

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

  async function handleUninstall(): Promise<boolean | undefined> {
    if (busy) {
      return false;
    }

    const result = await store.uninstall(gameId);
    // Shared confirm dialog expects boolean-ish success; map tri-state.
    return isMutationSuccess(result);
  }
</script>

<div class="flex w-full flex-col gap-4">
  <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
    <AddonFieldLabel label={t(labels.statusLabel)} class="flex-nowrap gap-1.5">
      <Badge variant="secondary">{t(labels.statusInstalled)}</Badge>
    </AddonFieldLabel>

    <AddonFieldLabel label={t(labels.freshnessLabel)} class="flex-nowrap gap-1.5">
      <AddonToolStatusBadge status={store.freshness} i18nPrefix={statusI18nPrefix} />
    </AddonFieldLabel>
  </div>

  <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
    {#if addonDateLabel}
      <span class="text-foreground/80">
        <CalendarIcon class="me-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
        {t(labels.addonDated, { date: addonDateLabel })}
      </span>
    {/if}

    {#if installedLabel}
      <span>{t(labels.installedOn, { date: installedLabel })}</span>
    {/if}

    <span>
      <ClockIcon class="me-1 inline size-3.5 align-text-bottom" aria-hidden="true" />
      {checkedLabel ? t(labels.lastChecked, { time: checkedLabel }) : t(labels.lastCheckedNever)}
    </span>
  </div>

  {@render topWarnings?.()}

  {@render afterDateCallouts?.()}

  <ItemGroup class="rounded-md border bg-muted/30">
    <AddonComponentRow
      icon="reshade"
      title={t(labels.componentReshade)}
      description={reshadeDescription}
      status={store.hostUpdate}
      {statusI18nPrefix}
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
      {statusI18nPrefix}
    />

    {@render extraComponentRows?.()}
  </ItemGroup>

  {@render afterComponents?.()}

  {#if primaryHostDisabledMessage}
    <AddonStateMessage tone="warning" icon="warning" message={primaryHostDisabledMessage} />
  {/if}

  <div class="flex flex-wrap items-center justify-between gap-2 px-1">
    {@render actionRowLeading?.()}

    <div class="ms-auto flex flex-wrap items-center gap-2">
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
</div>
