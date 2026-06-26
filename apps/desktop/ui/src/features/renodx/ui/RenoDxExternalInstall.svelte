<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import { publishErrorNotification } from '@shared/notifications';
  import { Badge, Button } from '@shared/ui';

  import { ADDON_EXTENSIONS, isAddonFile } from '../model/validate-addon';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';
  import type { MatchConfidence } from '../model/types';
  import type { RenoDxStore } from '../model/create-renodx-store.svelte';

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

  const CONFIDENCE_LABELS = {
    verified: 'gameDetails.renodx.confidenceVerified',
    experimental: 'gameDetails.renodx.confidenceExperimental',
    untested: 'gameDetails.renodx.confidenceUntested',
  } as const satisfies Record<MatchConfidence, string>;
  const confidenceVariant = (confidence: MatchConfidence | null): 'secondary' | 'outline' =>
    confidence === 'verified' ? 'secondary' : 'outline';

  const externalLabel = $derived(
    store.externalLabelKey
      ? translateKey(store.externalLabelKey, t('gameDetails.renodx.actionOpenExternal'))
      : t('gameDetails.renodx.actionOpenExternal'),
  );
  const externalConfidenceLabel = $derived(
    store.externalConfidence ? t(CONFIDENCE_LABELS[store.externalConfidence]) : '',
  );
  const externalRiskFallback = $derived(
    store.externalIsBlocked
      ? t('gameDetails.renodx.riskBlocked')
      : store.externalRequiresConfirmation
        ? t('gameDetails.renodx.riskWarn')
        : t('gameDetails.renodx.riskSafe'),
  );
  const externalRiskText = $derived(
    store.externalRisk ? translateKey(store.externalRisk.message_key, externalRiskFallback) : '',
  );
  const externalNotes = $derived(
    store.externalNotes.map((key) =>
      translateKey(key, key.replace(/^.*\./, '').replace(/_/g, ' ')),
    ),
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
      void store.installFromFile(gameId, filePath, false);
    }
  }

  function confirmFileInstall(): void {
    const file = pendingFilePath;
    pendingFilePath = null;
    if (file) {
      void store.installFromFile(gameId, file, true);
    }
  }
</script>

<div
  bind:this={externalDropEl}
  role="region"
  aria-label={t('gameDetails.renodx.external.dropHint')}
  class="flex w-full flex-wrap items-center gap-3 rounded-md border-2 border-dashed p-3 transition-colors"
  class:border-primary={drop.dragActive}
  class:border-transparent={!drop.dragActive}
>
  {#if store.externalConfidence}
    <Badge variant={confidenceVariant(store.externalConfidence)}>
      {externalConfidenceLabel}
    </Badge>
  {/if}
  <span class="text-sm text-muted-foreground">{externalRiskText}</span>
  <div class="ml-auto flex items-center gap-2">
    <DownloadProgressBar ids={[gameId]} active={store.busy} />
    <Button variant="outline" size="sm" disabled={busy} onclick={openExternalLink}>
      {externalLabel}
    </Button>
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
  </div>
  <p class="w-full text-xs text-muted-foreground">{t('gameDetails.renodx.external.dropHint')}</p>
  {#if externalNotes.length > 0}
    <ul class="w-full list-inside list-disc text-xs text-muted-foreground">
      {#each externalNotes as note (note)}
        <li>{note}</li>
      {/each}
    </ul>
  {/if}
  {#if pendingFilePath}
    <p class="w-full text-xs text-destructive" aria-live="polite">
      {t('gameDetails.renodx.confirmBody')}
    </p>
  {/if}
</div>
