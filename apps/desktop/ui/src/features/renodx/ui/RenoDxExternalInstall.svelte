<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import { publishErrorNotification } from '@shared/notifications';
  import { Button } from '@shared/ui';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { humanizeMessageKey, riskFallbackKey } from '../model/reshade-presenters';
  import type { ReshadeChannel } from '../model/types';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';
  import { ADDON_EXTENSIONS, isAddonFile } from '../model/validate-addon';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxConfidenceBadge from './RenoDxConfidenceBadge.svelte';
  import RenoDxRiskConfirmDialog from './RenoDxRiskConfirmDialog.svelte';
  import RenoDxStateMessage from './RenoDxStateMessage.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag: some exclusive operation is already in flight. */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  let externalDropEl = $state<HTMLElement | null>(null);
  let pendingFilePath = $state<string | null>(null);

  const drop = createAddonDrop(() => externalDropEl, handleDroppedPaths);

  const progressIds = $derived([gameId]);

  const isActionBusy = $derived(busy || store.busy);
  const canPickFile = $derived(!store.externalIsBlocked && !isActionBusy);
  const canOpenExternal = $derived(Boolean(store.externalUrl) && !isActionBusy);

  const dropHint = $derived(t('gameDetails.renodx.external.dropHint'));
  const installFromFileLabel = $derived(t('gameDetails.renodx.external.installFromFile'));
  const invalidFileLabel = $derived(t('gameDetails.renodx.external.invalidFile'));

  const fileInstallLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : installFromFileLabel,
  );

  const externalLabel = $derived(
    store.externalLabelKey
      ? translateKey(store.externalLabelKey, t('gameDetails.renodx.actionOpenExternal'))
      : t('gameDetails.renodx.actionOpenExternal'),
  );

  const externalRiskText = $derived(
    store.externalRisk
      ? translateKey(
          store.externalRisk.message_key,
          t(riskFallbackKey(store.externalRisk.severity)),
        )
      : null,
  );

  const externalNotes = $derived.by(() =>
    store.externalNotes.map((key) => translateKey(key, humanizeMessageKey(key))),
  );

  const showHostChannelControl = $derived(
    store.externalFileInstallable &&
      store.outcome?.kind === 'external' &&
      store.outcome.file_install?.host_kind === 'proxy',
  );

  const confirmOpen = $derived(pendingFilePath !== null);

  function formatError(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }

    if (typeof error === 'string') {
      return error;
    }

    return String(error);
  }

  function reportError(title: string, error: unknown): void {
    publishErrorNotification(title, formatError(error));
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
    try {
      await store.installFromFile(gameId, filePath, store.selectedReshadeChannel, confirmedRisk);
    } catch (error) {
      reportError(installFromFileLabel, error);
    }
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
    <div>
      <RenoDxConfidenceBadge confidence={store.externalConfidence} />
    </div>
  {/if}

  {#if externalRiskText}
    <p class="text-sm text-muted-foreground">{externalRiskText}</p>
  {/if}

  {#if store.externalRequiresConfirmation}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.fullAddonWarning')}
    />
  {/if}

  {#if externalNotes.length > 0}
    <ul class="list-inside list-disc text-sm text-muted-foreground">
      {#each externalNotes as note (note)}
        <li>{note}</li>
      {/each}
    </ul>
  {/if}

  <p class="text-sm text-muted-foreground">{dropHint}</p>

  <div class="flex flex-wrap items-center gap-2">
    <DownloadProgressBar ids={progressIds} active={store.busy} />

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

    {#if store.externalIsBlocked}
      <Button size="sm" disabled>{installFromFileLabel}</Button>
    {:else}
      <Button size="sm" disabled={!canPickFile} onclick={pickFile}>
        {fileInstallLabel}
      </Button>
    {/if}

    <Button variant="outline" size="sm" disabled={!canOpenExternal} onclick={openExternalLink}>
      {externalLabel}
    </Button>
  </div>
</div>

<RenoDxRiskConfirmDialog
  open={confirmOpen}
  busy={isActionBusy}
  riskText={externalRiskText ?? ''}
  onOpenChange={setFileConfirmOpen}
  onConfirm={confirmFileInstall}
/>
