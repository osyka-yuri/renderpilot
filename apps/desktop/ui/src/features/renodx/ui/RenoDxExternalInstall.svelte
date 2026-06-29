<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import { publishErrorNotification } from '@shared/notifications';
  import { Button } from '@shared/ui';

  import { ADDON_EXTENSIONS, isAddonFile } from '../model/validate-addon';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';
  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import { humanizeMessageKey, riskFallbackKey } from '../model/reshade-presenters';
  import type { ReshadeChannel } from '../model/types';
  import RenoDxChannelSelect from './RenoDxChannelSelect.svelte';
  import RenoDxConfidenceBadge from './RenoDxConfidenceBadge.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag (any exclusive operation in flight). */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  // The external drop-zone element.
  let externalDropEl = $state<HTMLElement | null>(null);
  // A picked/dropped file awaiting anti-cheat confirmation (warn-risk external).
  let pendingFilePath = $state<string | null>(null);

  const drop = createAddonDrop(() => externalDropEl, handleDroppedPaths);

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
      : '',
  );
  const externalNotes = $derived(
    store.externalNotes.map((key) => translateKey(key, humanizeMessageKey(key))),
  );
  const fileInstallLabel = $derived(
    store.busy
      ? t('gameDetails.renodx.installing')
      : t('gameDetails.renodx.external.installFromFile'),
  );

  async function openExternalLink(): Promise<void> {
    if (store.externalUrl) {
      await openExternal(store.externalUrl);
    }
  }

  async function pickFile(): Promise<void> {
    const { openFilePicker } = await import('@shared/api');
    const file = await openFilePicker({
      filters: [{ name: 'RenoDX add-on', extensions: [...ADDON_EXTENSIONS] }],
    });
    if (file) {
      beginFileInstall(file);
    }
  }

  function handleDroppedPaths(paths: string[]): void {
    const file = paths.find(isAddonFile);
    if (!file) {
      publishErrorNotification(t('gameDetails.renodx.external.invalidFile'), paths.join('\n'));
      return;
    }
    beginFileInstall(file);
  }

  // Warn-risk external titles step through confirmation; otherwise install directly.
  function beginFileInstall(filePath: string): void {
    if (store.externalRequiresConfirmation) {
      pendingFilePath = filePath;
    } else {
      void store.installFromFile(gameId, filePath, store.selectedReshadeChannel, false);
    }
  }

  function confirmFileInstall(): void {
    const file = pendingFilePath;
    pendingFilePath = null;
    if (file) {
      void store.installFromFile(gameId, file, store.selectedReshadeChannel, true);
    }
  }

  function selectChannel(channel: ReshadeChannel): void {
    store.setSelectedReshadeChannel(channel);
  }
</script>

<div
  bind:this={externalDropEl}
  role="region"
  aria-label={t('gameDetails.renodx.external.dropHint')}
  class="flex w-full flex-col gap-3 rounded-md transition-shadow"
  class:ring-2={drop.dragActive}
  class:ring-primary={drop.dragActive}
>
  {#if store.externalConfidence}
    <div>
      <RenoDxConfidenceBadge confidence={store.externalConfidence} />
    </div>
  {/if}

  <p class="text-sm text-muted-foreground">{externalRiskText}</p>

  {#if externalNotes.length > 0}
    <ul class="list-inside list-disc text-xs text-muted-foreground">
      {#each externalNotes as note (note)}
        <li>{note}</li>
      {/each}
    </ul>
  {/if}

  <p class="text-xs text-muted-foreground">{t('gameDetails.renodx.external.dropHint')}</p>

  <RenoDxChannelSelect
    value={store.selectedReshadeChannel}
    stableSupported={store.reshadeStableSupported}
    disabled={busy}
    onValueChange={selectChannel}
  />

  {#if pendingFilePath}
    <p class="text-xs text-destructive" aria-live="polite">
      {t('gameDetails.renodx.confirmBody')}
    </p>
  {/if}

  <div class="flex flex-wrap items-center gap-2">
    <DownloadProgressBar ids={[gameId]} active={store.busy} />
    {#if store.externalIsBlocked}
      <Button size="sm" disabled>{t('gameDetails.renodx.external.installFromFile')}</Button>
    {:else if pendingFilePath}
      <Button size="sm" variant="outline" onclick={() => (pendingFilePath = null)}>
        {t('gameDetails.renodx.cancel')}
      </Button>
      <Button size="sm" variant="destructive" disabled={busy} onclick={confirmFileInstall}>
        {t('gameDetails.renodx.confirmAccept')}
      </Button>
    {:else}
      <Button size="sm" disabled={busy} onclick={pickFile}>{fileInstallLabel}</Button>
    {/if}
    <Button variant="outline" size="sm" disabled={busy} onclick={openExternalLink}>
      {externalLabel}
    </Button>
  </div>
</div>
