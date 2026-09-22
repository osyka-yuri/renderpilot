<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import Settings2Icon from '@lucide/svelte/icons/settings-2';
  import WrenchIcon from '@lucide/svelte/icons/wrench';
  import { untrack } from 'svelte';
  import { cn } from '@shared/classnames';

  import {
    AddonAttribution,
    AddonCardFooter,
    AddonCardShell,
    AddonFieldLabel,
    AddonSignalBadge,
    AddonStateMessage,
    AddonUninstallAction,
    isMutationSuccess,
  } from '@entities/addon';
  import { t, translateExternalMessage, type MessageKeyWithoutParams } from '@shared/i18n';
  import { Badge, Button, LaunchArgumentsCallout, Spinner } from '@shared/ui';

  import { createOptiScalerCardController } from '../model/create-optiscaler-card-controller.svelte';
  import {
    createOptiScalerStore,
    type OptiScalerStore,
  } from '../model/create-optiscaler-store.svelte';
  import {
    formatOptiScalerDeclaredInput,
    optiscalerActionAvailability,
    optiscalerBlockedAlertPresentation,
    optiscalerCardState,
    optiscalerCompatibilityBadgeTone,
    optiscalerInstallVisible,
    trustedOptiScalerGuidance,
  } from '../model/presentation';
  import type {
    OptiScalerAvailability,
    OptiScalerCompatibilityBlockCode,
    OptiScalerCompatibilityStatus,
    OptiScalerPrerequisiteState,
  } from '../model/types';
  import OptiScalerSettingsDialog from './OptiScalerSettingsDialog.svelte';

  type Props = { gameId: string; busy?: boolean; store?: OptiScalerStore };

  const { gameId, busy: pageBusy = false, store: injectedStore }: Props = $props();
  const initialStore = untrack(() => injectedStore);
  const store = initialStore ?? createOptiScalerStore();
  const ownsStore = initialStore === undefined;
  const controller = createOptiScalerCardController(() => gameId, store);

  const attribution = {
    textKey: 'gameDetails.optiscaler.attribution' as const,
    linkKey: 'gameDetails.optiscaler.attributionLink' as const,
    href: 'https://github.com/optiscaler/OptiScaler',
  };

  const compatibilityStatusKeys = {
    working: 'gameDetails.optiscaler.compatibilityVerified',
    conditional: 'gameDetails.optiscaler.compatibilityConditional',
    untested: 'gameDetails.optiscaler.compatibilityUntested',
    unsupported: 'gameDetails.optiscaler.compatibilityUnsupported',
  } as const satisfies Record<OptiScalerCompatibilityStatus, MessageKeyWithoutParams>;

  const blockMessageKeys: Record<OptiScalerCompatibilityBlockCode, MessageKeyWithoutParams> = {
    requires_x64: 'gameDetails.optiscaler.block.requiresX64',
    unsupported_graphics_api: 'gameDetails.optiscaler.block.unsupportedApi',
    catalog_unsupported: 'gameDetails.optiscaler.block.catalogUnsupported',
    input_not_detected: 'gameDetails.optiscaler.block.generic',
    release_unavailable: 'gameDetails.optiscaler.block.releaseUnavailable',
    selected_modules_unavailable: 'gameDetails.optiscaler.block.modulesUnavailable',
    catalog_identity_conflict: 'gameDetails.optiscaler.block.generic',
    luma_prerequisite: 'gameDetails.optiscaler.block.generic',
    proxy_conflict: 'gameDetails.optiscaler.block.proxyConflict',
  };

  const prerequisiteMessageKeys: Partial<
    Record<OptiScalerPrerequisiteState, MessageKeyWithoutParams>
  > = {
    remove_reno_dx: 'gameDetails.optiscaler.prerequisite.removeRenoDx',
    luma_torn: 'gameDetails.optiscaler.prerequisite.lumaTorn',
    luma_broken: 'gameDetails.optiscaler.prerequisite.lumaBroken',
    install_luma: 'gameDetails.optiscaler.prerequisite.installLuma',
    luma_unavailable: 'gameDetails.optiscaler.prerequisite.lumaUnavailable',
  };

  $effect(() => {
    if (ownsStore) {
      void store.load(gameId);
    }
  });

  const combinedBusy = $derived(pageBusy || store.busy || store.loading);
  const installed = $derived(store.state !== null);
  const report = $derived(store.report);
  const cardState = $derived(report ? optiscalerCardState(report) : null);
  const actionAvailability = $derived(
    report ? optiscalerActionAvailability(report, installed, combinedBusy) : null,
  );
  const installVisible = $derived(report ? optiscalerInstallVisible(report, installed) : false);
  const installLabel = $derived(
    store.busy ? t('gameDetails.optiscaler.installing') : t('gameDetails.optiscaler.install'),
  );
  const version = $derived(
    report ? (report.install.release ?? report.selected_release ?? '—') : '—',
  );
  const declaredInputs = $derived(
    report ? report.compatibility.declared_inputs.map(formatOptiScalerDeclaredInput) : [],
  );
  const guidanceMessages = $derived(
    report
      ? report.compatibility.guidance.flatMap((guidance) => {
          const message = trustedOptiScalerGuidance(guidance);
          return message === null ? [] : [message];
        })
      : [],
  );
  function blockingMessage(current: OptiScalerAvailability): string {
    if (current.proxy_conflict) {
      return t('gameDetails.optiscaler.block.proxyConflict');
    }
    return current.eligibility.block_code
      ? t(blockMessageKeys[current.eligibility.block_code])
      : t('gameDetails.optiscaler.block.generic');
  }

  function prerequisiteMessage(current: OptiScalerAvailability): string | null {
    const key = prerequisiteMessageKeys[current.prerequisite.state];
    return key ? t(key) : null;
  }
</script>

<AddonCardShell
  title={t('gameDetails.optiscaler.title')}
  description={t('gameDetails.optiscaler.description')}
  loadingLabel={t('gameDetails.optiscaler.loading')}
  progressIds={[gameId]}
  progressActive={store.busy}
  actionsDisabled={combinedBusy}
  showLoading={store.loading && !store.loaded}
  showLoadError={!!store.loadError && !store.loaded}
  retrying={store.loading}
  showAttribution={false}
  {attribution}
  onRetry={() => {
    void store.retry(gameId);
  }}
>
  {#if report && cardState}
    <div class="flex w-full flex-1 flex-col gap-4 text-sm">
      <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
        {#if installed || cardState === 'unmanaged'}
          <AddonFieldLabel label={t('gameDetails.optiscaler.statusLabel')}>
            <Badge
              variant={installed ? 'secondary' : 'outline'}
              class={cardState === 'unmanaged' ? 'border-warning/40 bg-warning/10' : ''}
            >
              {t(
                installed
                  ? 'gameDetails.optiscaler.statusInstalled'
                  : 'gameDetails.optiscaler.statusUnmanaged',
              )}
            </Badge>
          </AddonFieldLabel>
        {/if}
        <AddonSignalBadge
          tone={optiscalerCompatibilityBadgeTone(report.compatibility.status)}
          fieldLabel={t('gameDetails.optiscaler.compatibilityLabel')}
          valueLabel={t(compatibilityStatusKeys[report.compatibility.status])}
        />
        <AddonFieldLabel label={t('gameDetails.optiscaler.versionLabel')}>
          <Badge variant="outline">{version}</Badge>
        </AddonFieldLabel>
        {#if declaredInputs.length > 0}
          <AddonFieldLabel label={t('gameDetails.optiscaler.integrationThrough')}>
            <span class="flex flex-wrap gap-1">
              {#each declaredInputs as input, index (index)}
                <Badge variant="outline">{input}</Badge>
              {/each}
            </span>
          </AddonFieldLabel>
        {/if}
      </div>

      {#if cardState === 'unmanaged'}
        <AddonStateMessage
          tone="warning"
          icon="warning"
          message={t('gameDetails.optiscaler.unmanaged')}
        />
      {:else if cardState === 'blocked'}
        {@const alert = optiscalerBlockedAlertPresentation(report)}
        <AddonStateMessage
          tone={alert.tone}
          icon={alert.icon}
          message={prerequisiteMessage(report) ?? blockingMessage(report)}
        />
      {:else if report.proxy_conflict}
        <AddonStateMessage tone="warning" icon="warning" message={blockingMessage(report)} />
      {/if}

      {#if !installed && cardState !== 'blocked' && prerequisiteMessage(report) !== null}
        {@const alert = optiscalerBlockedAlertPresentation(report)}
        <AddonStateMessage
          tone={alert.tone}
          icon={alert.icon}
          message={prerequisiteMessage(report) ?? ''}
        />
      {/if}

      {#if installed && !report.lifecycle.maintenance_available && report.lifecycle.maintenance_block_code}
        <AddonStateMessage
          tone="warning"
          icon="warning"
          message={t(blockMessageKeys[report.lifecycle.maintenance_block_code])}
        />
      {/if}

      {#if report.lifecycle.drifted}
        <AddonStateMessage
          tone="warning"
          icon="warning"
          message={t('gameDetails.optiscaler.drifted')}
        />
      {/if}

      {#if report.relocation}
        <div
          class="flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-muted/30 p-3"
        >
          <div class="min-w-0">
            <p class="font-medium">{t('gameDetails.optiscaler.relocationAvailable')}</p>
            <p class="mt-0.5 text-xs text-muted-foreground">
              {report.relocation.display_name}
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!actionAvailability?.relocate}
            onclick={() => {
              controller.requestAction('relocate');
            }}
          >
            {t('gameDetails.optiscaler.relocate')}
          </Button>
        </div>
      {/if}

      <LaunchArgumentsCallout launch={report.compatibility.launch} launcher={report.launcher} />

      {#each guidanceMessages as message (message.id)}
        <AddonStateMessage
          tone="default"
          icon="info"
          message={translateExternalMessage({
            key: message.id,
            fallback: message.fallback_text,
          })}
        />
      {/each}

      <AddonCardFooter>
        {#snippet leading()}
          <AddonAttribution {...attribution} />
        {/snippet}

        {#snippet actions()}
          {#if report.modules.length > 0 && cardState !== 'blocked' && cardState !== 'unmanaged'}
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={combinedBusy || (installed && !actionAvailability?.modules)}
              onclick={() => {
                controller.settingsOpen = true;
              }}
            >
              <Settings2Icon class="size-4" aria-hidden="true" />
              {installed
                ? t('gameDetails.optiscaler.configure')
                : t('gameDetails.optiscaler.configureBeforeInstall')}
            </Button>
          {/if}

          {#if installed}
            {#if store.updateAvailable}
              <Button
                type="button"
                size="sm"
                disabled={!actionAvailability?.update}
                onclick={() => {
                  controller.requestAction('update');
                }}
              >
                <DownloadIcon class="size-4" aria-hidden="true" />
                {t('gameDetails.optiscaler.update')}
              </Button>
            {:else if store.repairRequired}
              <Button
                type="button"
                size="sm"
                disabled={!actionAvailability?.repair}
                onclick={() => {
                  controller.requestAction('repair');
                }}
              >
                <WrenchIcon class="size-4" aria-hidden="true" />
                {t('gameDetails.optiscaler.repair')}
              </Button>
            {:else}
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={combinedBusy}
                aria-busy={store.checkingUpdates}
                onclick={() => {
                  void store.checkForUpdates(gameId);
                }}
              >
                <RefreshCwIcon
                  class={cn('size-4', store.checkingUpdates && 'animate-spin')}
                  aria-hidden="true"
                />
                {t('gameDetails.optiscaler.checkUpdates')}
              </Button>
              <span class="sr-only" role="status">
                {#if store.checkingUpdates}
                  {t('addon.availability.checking')}
                {/if}
              </span>
            {/if}

            <AddonUninstallAction
              busy={combinedBusy}
              disabled={!actionAvailability?.uninstall}
              actionKey="gameDetails.optiscaler.uninstall"
              confirmTitleKey="gameDetails.optiscaler.uninstallConfirmTitle"
              confirmBodyKey="gameDetails.optiscaler.uninstallConfirmBody"
              confirmActionKey="gameDetails.optiscaler.uninstallConfirmAction"
              onConfirm={async () => isMutationSuccess(await store.uninstall(gameId))}
            />
          {:else if installVisible}
            <Button
              type="button"
              size="sm"
              disabled={!actionAvailability?.install}
              onclick={() => {
                controller.requestAction('install');
              }}
            >
              {#if store.busy}
                <Spinner class="size-4" />
              {:else}
                <DownloadIcon class="size-4" aria-hidden="true" />
              {/if}
              {installLabel}
            </Button>
          {/if}
        {/snippet}
      </AddonCardFooter>
    </div>

    <OptiScalerSettingsDialog
      open={controller.settingsOpen}
      {gameId}
      {report}
      selected={controller.selectedModules}
      busy={combinedBusy}
      onOpenChange={(open: boolean) => {
        controller.settingsOpen = open;
      }}
      onSave={controller.saveModules}
    />
  {/if}
</AddonCardShell>
