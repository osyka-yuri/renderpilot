<script lang="ts">
  import { DownloadProgressBar } from '@entities/library';
  import { t, translateKey } from '@shared/i18n';
  import { Button, Spinner } from '@shared/ui';
  import DownloadIcon from '@lucide/svelte/icons/download';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import {
    actionDisabledMessage,
    humanizeMessageKey,
    riskFallbackKey,
  } from '../model/reshade-presenters';
  import type { ReshadeChannel } from '../model/types';
  import RenoDxChannelControl from './RenoDxChannelControl.svelte';
  import RenoDxConfidenceBadge from './RenoDxConfidenceBadge.svelte';
  import RenoDxRiskConfirmDialog from './RenoDxRiskConfirmDialog.svelte';
  import RenoDxStateMessage from './RenoDxStateMessage.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag: any exclusive page-level operation is in flight. */
    busy: boolean;
  };

  type RenoDxNote = {
    key: string;
    message: string;
  };

  const { gameId, store, busy }: Props = $props();

  let confirmOpen = $state(false);

  const progressIds = $derived([gameId]);
  const hostConflict = $derived(store.hostDetection === 'conflict');
  // A recognized custom build (e.g. GShade) is reported as a conflict at the
  // backend (the slot is never safe to write to), but it isn't a problem to
  // resolve — the install button is disabled the same way, with a plain label
  // instead of the generic "conflict, resolve it" wording.
  const customBuild = $derived(store.hostFacts.is_custom_build);

  const installHostKind = $derived(
    store.outcome?.kind === 'installable' ? store.outcome.host_kind : null,
  );

  const showDxChannelControl = $derived(installHostKind === 'proxy');

  const installAction = $derived(store.hostActions.install);
  const installDisabledByHost = $derived(installAction?.enabled === false);
  const installDisabledMessage = $derived(actionDisabledMessage(installAction) ?? '');

  const installBlocked = $derived(store.isBlocked || installDisabledByHost || customBuild);
  const canStartInstall = $derived(store.isInstallable && !busy && !installBlocked);

  const riskText = $derived.by((): string => {
    const risk = store.risk;

    if (!risk) {
      return '';
    }

    return translateKey(risk.message_key, t(riskFallbackKey(risk.severity)));
  });

  const showRiskText = $derived(riskText.length > 0);
  const showRiskAsWarning = $derived(store.isBlocked || store.requiresConfirmation);

  const hasHostInstallOrMaintenanceAction = $derived.by((): boolean => {
    const { install, repair, update } = store.hostActions;

    return install !== undefined || repair !== undefined || update !== undefined;
  });

  const showFullAddonWarning = $derived(
    store.requiresConfirmation &&
      (store.hostFacts.addon_support === 'full' || hasHostInstallOrMaintenanceAction),
  );

  const notes = $derived.by((): RenoDxNote[] =>
    store.notesKeys.map((key) => ({
      key,
      message: translateKey(key, humanizeMessageKey(key)),
    })),
  );

  const installLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : t('gameDetails.renodx.actionInstall'),
  );

  function setChannel(channel: ReshadeChannel): void {
    store.setSelectedReshadeChannel(channel);
  }

  function startInstall(): void {
    if (!canStartInstall) {
      return;
    }

    if (store.requiresConfirmation) {
      confirmOpen = true;
      return;
    }

    install(false);
  }

  function installConfirmed(): void {
    confirmOpen = false;
    install(true);
  }

  function install(force: boolean): void {
    if (!canStartInstall) {
      return;
    }

    void store.install(gameId, store.selectedReshadeChannel, force);
  }

  function setInstallConfirmOpen(nextOpen: boolean): void {
    confirmOpen = nextOpen;
  }
</script>

<div class="flex w-full flex-col gap-3">
  {#if store.confidence}
    <div>
      <RenoDxConfidenceBadge confidence={store.confidence} />
    </div>
  {/if}

  {#if customBuild}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.host.customBuild')}
    />
  {:else if hostConflict}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.host.conflictBlocksInstall')}
    />
  {/if}

  {#if showRiskText}
    {#if showRiskAsWarning}
      <RenoDxStateMessage tone="warning" icon="warning" message={riskText} />
    {:else}
      <p class="text-sm text-muted-foreground">{riskText}</p>
    {/if}
  {/if}

  {#if showFullAddonWarning}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.fullAddonWarning')}
    />
  {/if}

  {#if notes.length > 0}
    <ul class="list-inside list-disc text-sm text-muted-foreground">
      {#each notes as note (note.key)}
        <li>{note.message}</li>
      {/each}
    </ul>
  {/if}

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

  {#if installDisabledMessage}
    <RenoDxStateMessage tone="warning" icon="warning" message={installDisabledMessage} />
  {/if}

  <div class="flex items-center gap-2">
    <DownloadProgressBar ids={progressIds} active={store.busy} />

    {#if installBlocked}
      <Button type="button" size="sm" disabled>
        {t('gameDetails.renodx.actionInstall')}
      </Button>
    {:else}
      <Button type="button" size="sm" disabled={busy} onclick={startInstall}>
        {#if store.busy}
          <Spinner class="size-4" />
        {:else}
          <DownloadIcon class="size-4" aria-hidden="true" />
        {/if}

        {installLabel}
      </Button>
    {/if}
  </div>
</div>

<RenoDxRiskConfirmDialog
  open={confirmOpen}
  {busy}
  {riskText}
  onOpenChange={setInstallConfirmOpen}
  onConfirm={installConfirmed}
/>
