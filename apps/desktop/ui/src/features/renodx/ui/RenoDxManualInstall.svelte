<script lang="ts">
  import { t, translateMessageRef } from '@shared/i18n';
  import { Button } from '@shared/ui';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { ADDON_EXTENSIONS, isAddonFile, validateAddonFile } from '../model/validate-addon';
  import { createAddonDrop } from '../model/use-addon-drop.svelte';

  import type { ReshadeChannel } from '@entities/addon';

  import type { ManualFileInstall } from '../model/types';
  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    manual: ManualFileInstall;
    /** Global busy flag: any exclusive RenoDX operation is in flight. */
    busy: boolean;
  };

  const { gameId, store, manual, busy }: Props = $props();

  let dropEl = $state<HTMLElement | null>(null);
  let pendingPath = $state<string | null>(null);

  const showDxChannelControl = $derived(manual.host_kind === 'proxy');
  const expectedAddonName = $derived(manual.expected_addon_name ?? null);

  const canReview = $derived(!busy);

  const pendingValidation = $derived(
    pendingPath === null
      ? null
      : validateAddonFile(pendingPath, {
          gameArch: manual.game_arch,
          expectedAddonName: manual.expected_addon_name,
        }),
  );

  const pendingError = $derived(pendingValidation?.error ?? null);
  const pendingWarning = $derived(pendingValidation?.warning ?? null);
  const pendingFileName = $derived(pendingValidation?.fileName ?? '');

  const canConfirmInstall = $derived(canReview && pendingPath !== null && pendingError === null);

  const installLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : t('gameDetails.renodx.actionInstall'),
  );

  const drop = createAddonDrop(() => dropEl, handleDropped);
  const dropActive = $derived(canReview && drop.dragActive);

  function firstReviewablePath(paths: readonly string[]): string | null {
    const addonPath = paths.find(isAddonFile);

    if (addonPath !== undefined) {
      return addonPath;
    }

    if (paths.length === 0) {
      return null;
    }

    return paths[0];
  }

  function review(path: string): void {
    if (!canReview) {
      return;
    }

    pendingPath = path;
  }

  function handleDropped(paths: string[]): void {
    const path = firstReviewablePath(paths);

    if (path === null) {
      return;
    }

    review(path);
  }

  function chooseFile(): void {
    if (!canReview) {
      return;
    }

    void pickFile();
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
    const path = pendingPath;

    if (!canConfirmInstall || path === null) {
      return;
    }

    pendingPath = null;
    void store.installFromFile(gameId, path, store.selectedReshadeChannel);
  }

  function cancelReview(): void {
    pendingPath = null;
  }

  function setChannel(channel: ReshadeChannel): void {
    if (channel === store.selectedReshadeChannel) {
      return;
    }

    store.setSelectedReshadeChannel(channel);
  }
</script>

<div
  bind:this={dropEl}
  role="region"
  aria-label={t('gameDetails.renodx.fileInstall.title')}
  class="flex w-full flex-col gap-3 rounded-md transition-shadow"
  class:ring-2={dropActive}
  class:ring-primary={dropActive}
>
  <p class="text-sm font-medium">{t('gameDetails.renodx.fileInstall.title')}</p>

  {#if pendingPath !== null}
    {#if pendingError}
      <p class="flex items-center gap-1 text-sm text-destructive" aria-live="polite">
        <TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
        {translateMessageRef(pendingError)}
      </p>

      <div>
        <Button variant="outline" size="sm" disabled={!canReview} onclick={chooseFile}>
          {t('gameDetails.renodx.fileInstall.chooseAnother')}
        </Button>
      </div>
    {:else}
      <p class="text-sm" aria-live="polite">
        {t('gameDetails.renodx.fileInstall.confirm', { fileName: pendingFileName })}
      </p>

      {#if expectedAddonName}
        <p class="text-sm text-muted-foreground">
          {t('gameDetails.renodx.fileInstall.expected', { name: expectedAddonName })}
        </p>
      {/if}

      {#if pendingWarning}
        <p class="flex items-center gap-1 text-xs text-amber-600 dark:text-amber-500">
          <TriangleAlertIcon class="size-3.5 shrink-0" aria-hidden="true" />
          {translateMessageRef(pendingWarning)}
        </p>
      {/if}

      <div class="flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" onclick={cancelReview}>
          {t('gameDetails.renodx.cancel')}
        </Button>

        {#if showDxChannelControl}
          <RenoDxChannelControl
            class="max-w-72"
            value={store.selectedReshadeChannel}
            stableSupported={store.reshadeStableSupported}
            {busy}
            label={t('gameDetails.renodx.channel.hostLabel')}
            onChange={setChannel}
          />
        {/if}

        <Button size="sm" disabled={!canConfirmInstall} onclick={confirmInstall}>
          {installLabel}
        </Button>
      </div>
    {/if}
  {:else}
    {#if expectedAddonName}
      <p class="text-sm text-muted-foreground">
        {t('gameDetails.renodx.fileInstall.expected', { name: expectedAddonName })}
      </p>
    {/if}

    <div>
      {#if showDxChannelControl}
        <div class="mb-2">
          <RenoDxChannelControl
            class="max-w-72"
            value={store.selectedReshadeChannel}
            stableSupported={store.reshadeStableSupported}
            {busy}
            label={t('gameDetails.renodx.channel.hostLabel')}
            onChange={setChannel}
          />
        </div>
      {/if}

      <Button size="sm" disabled={!canReview} onclick={chooseFile}>
        {t('gameDetails.renodx.fileInstall.chooseFile')}
      </Button>
    </div>
  {/if}

  <p class="text-sm text-muted-foreground">
    {t('gameDetails.renodx.external.dropHint')}
  </p>
</div>
