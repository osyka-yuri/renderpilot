<script lang="ts">
  import { t, translateKey } from '@shared/i18n';
  import { Button } from '@shared/ui';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import {
    ADDON_EXTENSIONS,
    isAddonFile,
    validateAddonFile,
    type AddonValidation,
  } from '../model/validate-addon';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';
  import type { ManualFileInstall, ReshadeChannel } from '../model/types';
  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxChannelSelect from './RenoDxChannelSelect.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    manual: ManualFileInstall;
    /** Global busy flag (any exclusive operation in flight). */
    busy: boolean;
  };

  const { gameId, store, manual, busy }: Props = $props();

  let dropEl = $state<HTMLElement | null>(null);
  // The picked file pending confirmation, with its tiered validation result.
  let pending = $state<{ path: string; validation: AddonValidation } | null>(null);

  const blocked = $derived(manual.risk.severity === 'block');
  const riskText = $derived(
    translateKey(manual.risk.message_key, t('gameDetails.renodx.riskSafe')),
  );

  const drop = createAddonDrop(() => dropEl, handleDropped);

  function review(path: string): void {
    pending = {
      path,
      validation: validateAddonFile(path, {
        gameArch: manual.game_arch,
        expectedAddonName: manual.expected_addon_name,
      }),
    };
  }

  function handleDropped(paths: string[]): void {
    const first = paths.find(isAddonFile) ?? paths[0];
    if (first) {
      review(first);
    }
  }

  async function pickFile(): Promise<void> {
    const { openFilePicker } = await import('@shared/api');
    const file = await openFilePicker({
      filters: [{ name: 'RenoDX add-on', extensions: [...ADDON_EXTENSIONS] }],
    });
    if (file) {
      review(file);
    }
  }

  function confirmInstall(): void {
    const path = pending?.path;
    pending = null;
    if (path) {
      // The explicit confirmation here also acknowledges the risk gate.
      void store.installFromFile(gameId, path, store.selectedReshadeChannel, true);
    }
  }

  function selectChannel(channel: ReshadeChannel): void {
    store.setSelectedReshadeChannel(channel);
  }
</script>

<div
  bind:this={dropEl}
  role="region"
  aria-label={t('gameDetails.renodx.external.dropHint')}
  class="flex w-full flex-col gap-3 rounded-md transition-shadow"
  class:ring-2={drop.dragActive}
  class:ring-primary={drop.dragActive}
>
  <p class="text-sm font-medium">{t('gameDetails.renodx.fileInstall.title')}</p>

  {#if blocked}
    <p class="flex items-center gap-1 text-sm text-destructive">
      <TriangleAlertIcon class="size-4" aria-hidden="true" />
      {t('gameDetails.renodx.riskBlocked')}
    </p>
  {:else if pending}
    {#if pending.validation.error}
      <p class="flex items-center gap-1 text-sm text-destructive" aria-live="polite">
        <TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
        {t(pending.validation.error.key, pending.validation.error.params)}
      </p>
      <div>
        <Button variant="outline" size="sm" disabled={busy} onclick={pickFile}>
          {t('gameDetails.renodx.fileInstall.chooseAnother')}
        </Button>
      </div>
    {:else}
      <p class="text-sm" aria-live="polite">
        {t('gameDetails.renodx.fileInstall.confirm', { fileName: pending.validation.fileName })}
      </p>
      {#if manual.expected_addon_name}
        <p class="text-xs text-muted-foreground">
          {t('gameDetails.renodx.fileInstall.expected', { name: manual.expected_addon_name })}
        </p>
      {/if}
      <RenoDxChannelSelect
        value={store.selectedReshadeChannel}
        stableSupported={store.reshadeStableSupported}
        disabled={busy}
        onValueChange={selectChannel}
      />
      {#if pending.validation.warning}
        <p class="flex items-center gap-1 text-xs text-amber-600 dark:text-amber-500">
          <TriangleAlertIcon class="size-3.5 shrink-0" aria-hidden="true" />
          {t(pending.validation.warning.key, pending.validation.warning.params)}
        </p>
      {/if}
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" onclick={() => (pending = null)}>
          {t('gameDetails.renodx.cancel')}
        </Button>
        <Button size="sm" disabled={busy} onclick={confirmInstall}>
          {store.busy ? t('gameDetails.renodx.installing') : t('gameDetails.renodx.actionInstall')}
        </Button>
      </div>
    {/if}
  {:else}
    {#if manual.expected_addon_name}
      <p class="text-xs text-muted-foreground">
        {t('gameDetails.renodx.fileInstall.expected', { name: manual.expected_addon_name })}
      </p>
    {/if}
    <p class="text-xs text-muted-foreground">{riskText}</p>
    <RenoDxChannelSelect
      value={store.selectedReshadeChannel}
      stableSupported={store.reshadeStableSupported}
      disabled={busy}
      onValueChange={selectChannel}
    />
    <div>
      <Button size="sm" disabled={busy} onclick={pickFile}>
        {t('gameDetails.renodx.fileInstall.chooseFile')}
      </Button>
    </div>
  {/if}

  <p class="text-xs text-muted-foreground">{t('gameDetails.renodx.external.dropHint')}</p>
</div>
