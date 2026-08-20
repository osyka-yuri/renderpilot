<script lang="ts">
  import {
    AddonAttribution,
    AddonConfidenceBadge,
    AddonRiskConfirmDialog,
    AddonStateMessage,
    createConfidenceLabelKeys,
  } from '@entities/addon';
  import { openExternal } from '@shared/api';
  import { t, translateExternalMessage } from '@shared/i18n';
  import {
    publishErrorNotification,
    publishPresentedErrorNotification,
  } from '@shared/notifications';
  import { Button } from '@shared/ui';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { RENODX_ATTRIBUTION } from '../model/attribution';
  import { riskMessage } from '../model/reshade-presenters';
  import type { ReshadeChannel } from '@entities/addon';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';
  import { ADDON_EXTENSIONS, isAddonFile } from '../model/validate-addon';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag: some exclusive operation is already in flight. */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const CONFIDENCE_LABEL_KEY = createConfidenceLabelKeys('gameDetails.renodx');

  let externalDropEl = $state<HTMLElement | null>(null);
  let pendingFilePath = $state<string | null>(null);

  const drop = createAddonDrop(() => externalDropEl, handleDroppedPaths);

  const isActionBusy = $derived(busy || store.busy);
  const canPickFile = $derived(!isActionBusy);
  const canOpenExternal = $derived(Boolean(store.externalUrl) && !isActionBusy);

  const dropHint = $derived(t('gameDetails.renodx.external.dropHint'));
  const installFromFileLabel = $derived(t('gameDetails.renodx.external.installFromFile'));
  const invalidFileLabel = $derived(t('gameDetails.renodx.external.invalidFile'));

  const fileInstallLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : installFromFileLabel,
  );

  const externalLabel = $derived(
    store.externalMessage
      ? translateExternalMessage({
          key: store.externalMessage.id,
          fallback: store.externalMessage.fallback_text,
        })
      : t('gameDetails.renodx.actionOpenExternal'),
  );

  const externalRiskText = $derived(store.externalRisk ? riskMessage(store.externalRisk) : null);

  const showHostChannelControl = $derived(
    store.externalFileInstallable &&
      store.outcome?.kind === 'external' &&
      store.outcome.file_install?.host_kind === 'proxy',
  );

  const confirmOpen = $derived(pendingFilePath !== null);

  function reportError(title: string, error: unknown): void {
    publishPresentedErrorNotification(title, error);
  }

  async function openExternalLink(): Promise<void> {
    const url = store.externalUrl;

    if (!url || isActionBusy) {
      return;
    }

    try {
      await openExternal(url);
    } catch (error) {
      reportError(externalLabel, error);
    }
  }

  async function pickFile(): Promise<void> {
    if (!canPickFile) {
      return;
    }

    try {
      const { openFilePicker } = await import('@shared/api');
      const filePath = await openFilePicker({
        filters: [{ name: 'RenoDX add-on', extensions: [...ADDON_EXTENSIONS] }],
      });

      if (filePath) {
        beginFileInstall(filePath);
      }
    } catch (error) {
      reportError(installFromFileLabel, error);
    }
  }

  function handleDroppedPaths(paths: string[]): void {
    if (!canPickFile) {
      return;
    }

    const filePath = paths.find(isAddonFile);

    if (!filePath) {
      publishErrorNotification(invalidFileLabel, paths.join('\n'));
      return;
    }

    beginFileInstall(filePath);
  }

  function beginFileInstall(filePath: string): void {
    if (!isAddonFile(filePath)) {
      publishErrorNotification(invalidFileLabel, filePath);
      return;
    }

    if (isActionBusy) {
      return;
    }

    if (store.externalRequiresConfirmation) {
      pendingFilePath = filePath;
      return;
    }

    void installFile(filePath, false);
  }

  async function installFile(filePath: string, confirmedRisk: boolean): Promise<void> {
    await store.installFromFile(gameId, filePath, store.selectedReshadeChannel, confirmedRisk);
  }

  function confirmFileInstall(): void {
    const filePath = pendingFilePath;

    if (!filePath || isActionBusy) {
      return;
    }

    pendingFilePath = null;
    void installFile(filePath, true);
  }

  function cancelFileInstall(): void {
    pendingFilePath = null;
  }

  function setFileConfirmOpen(next: boolean): void {
    if (!next) {
      cancelFileInstall();
    }
  }

  function setChannel(channel: ReshadeChannel): void {
    if (!isActionBusy) {
      store.setSelectedReshadeChannel(channel);
    }
  }
</script>

<div
  bind:this={externalDropEl}
  role="region"
  aria-label={dropHint}
  class="flex w-full flex-col gap-3 rounded-md transition-shadow"
  class:ring-2={drop.dragActive}
  class:ring-primary={drop.dragActive}
>
  {#if store.externalConfidence}
    <AddonConfidenceBadge
      confidence={store.externalConfidence}
      fieldLabel={t('gameDetails.renodx.confidenceLabel')}
      confidenceLabel={t(CONFIDENCE_LABEL_KEY[store.externalConfidence])}
    />
  {/if}

  {#if externalRiskText}
    <p class="text-sm text-muted-foreground">{externalRiskText}</p>
  {/if}

  {#if store.externalRequiresConfirmation}
    <AddonStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.addon.fullAddonWarning')}
    />
  {/if}

  <p class="text-sm text-muted-foreground">{dropHint}</p>

  <div class="flex flex-wrap items-center justify-between gap-2 px-1">
    <AddonAttribution {...RENODX_ATTRIBUTION} />

    <div class="ms-auto flex flex-wrap items-center gap-2">
      {#if showHostChannelControl}
        <RenoDxChannelControl
          class="max-w-72"
          value={store.selectedReshadeChannel}
          stableSupported={store.reshadeStableSupported}
          busy={isActionBusy}
          label={t('gameDetails.renodx.channel.hostLabel')}
          onChange={setChannel}
        />
      {/if}

      <div class="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={!canPickFile} onclick={pickFile}>
          {fileInstallLabel}
        </Button>

        <Button variant="outline" size="sm" disabled={!canOpenExternal} onclick={openExternalLink}>
          {externalLabel}
        </Button>
      </div>
    </div>
  </div>
</div>

<AddonRiskConfirmDialog
  open={confirmOpen}
  busy={isActionBusy}
  riskText={externalRiskText ?? ''}
  titleKey="gameDetails.renodx.confirmTitle"
  bodyKey="gameDetails.addon.confirmBody"
  acceptKey="gameDetails.addon.confirmAccept"
  onOpenChange={setFileConfirmOpen}
  onConfirm={confirmFileInstall}
/>
