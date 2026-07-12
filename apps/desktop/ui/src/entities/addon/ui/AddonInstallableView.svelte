<script lang="ts">
  import type { Snippet } from 'svelte';

  import AddonConfidenceBadge from './AddonConfidenceBadge.svelte';
  import AddonRiskConfirmDialog from './AddonRiskConfirmDialog.svelte';
  import AddonStateMessage from './AddonStateMessage.svelte';
  import { t, translateKey, type MessageKey } from '@shared/i18n';
  import { Button, Spinner } from '@shared/ui';
  import DownloadIcon from '@lucide/svelte/icons/download';

  import type { MatchConfidence } from '../model/types';
  import type { AddonInstallableLabels } from '../model/presenters';
  import { actionDisabledMessage, humanizeMessageKey } from '../model/presenters';
  import type { AddonStoreView } from '../model/store-view';

  type ViewStore = Pick<
    AddonStoreView,
    | 'busy'
    | 'isInstallable'
    | 'requiresConfirmation'
    | 'confidence'
    | 'notesKeys'
    | 'hostDetection'
    | 'hostFacts'
    | 'hostActions'
  >;

  type Note = { key: string; message: string };

  type Props = {
    gameId: string;
    store: ViewStore;
    busy: boolean;
    labels: AddonInstallableLabels;
    confidenceLabelKey: Record<MatchConfidence, MessageKey>;
    onInstall: (gameId: string, force: boolean) => void;
    riskText: string;
    preConflictWarnings?: Snippet;
    preNotesCallouts?: Snippet;
    downloadProgress?: Snippet;
  };

  const {
    gameId,
    store,
    busy,
    labels,
    confidenceLabelKey,
    onInstall,
    riskText,
    preConflictWarnings,
    preNotesCallouts,
    downloadProgress,
  }: Props = $props();

  let confirmOpen = $state(false);

  const hostConflict = $derived(store.hostDetection === 'conflict');
  const customBuild = $derived(store.hostFacts.is_custom_build);

  const installAction = $derived(store.hostActions.install);
  const installDisabledByHost = $derived(installAction?.enabled === false);
  const installDisabledMessage = $derived(actionDisabledMessage(installAction) ?? '');

  const installBlocked = $derived(installDisabledByHost || customBuild);
  const canStartInstall = $derived(store.isInstallable && !busy && !installBlocked);

  const showRiskText = $derived(riskText.length > 0);
  const showRiskAsWarning = $derived(store.requiresConfirmation);

  const hasHostInstallOrMaintenanceAction = $derived.by((): boolean => {
    const { install, repair, update } = store.hostActions;

    return install !== undefined || repair !== undefined || update !== undefined;
  });

  const showFullAddonWarning = $derived(
    store.requiresConfirmation &&
      (store.hostFacts.addon_support === 'full' || hasHostInstallOrMaintenanceAction),
  );

  const notes = $derived.by((): Note[] =>
    store.notesKeys.map((key) => ({
      key,
      message: translateKey(key, humanizeMessageKey(key)),
    })),
  );

  const installLabel = $derived(store.busy ? t(labels.installing) : t(labels.installAction));

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

    onInstall(gameId, force);
  }

  function setInstallConfirmOpen(nextOpen: boolean): void {
    confirmOpen = nextOpen;
  }
</script>

<div class="flex w-full flex-col gap-3">
  {#if store.confidence}
    <AddonConfidenceBadge
      confidence={store.confidence}
      fieldLabel={t(labels.confidenceLabel)}
      confidenceLabel={t(confidenceLabelKey[store.confidence])}
    />
  {/if}

  {@render preConflictWarnings?.()}

  {#if customBuild}
    <AddonStateMessage tone="warning" icon="warning" message={t(labels.hostCustomBuild)} />
  {:else if hostConflict}
    <AddonStateMessage
      tone="warning"
      icon="warning"
      message={t(labels.hostConflictBlocksInstall)}
    />
  {/if}

  {#if showRiskText}
    {#if showRiskAsWarning}
      <AddonStateMessage tone="warning" icon="warning" message={riskText} />
    {:else}
      <p class="text-sm text-muted-foreground">{riskText}</p>
    {/if}
  {/if}

  {#if showFullAddonWarning}
    <AddonStateMessage tone="warning" icon="warning" message={t(labels.fullAddonWarning)} />
  {/if}

  {@render preNotesCallouts?.()}

  {#if notes.length > 0}
    <ul class="list-inside list-disc text-sm text-muted-foreground">
      {#each notes as note (note.key)}
        <li>{note.message}</li>
      {/each}
    </ul>
  {/if}

  {#if installDisabledMessage}
    <AddonStateMessage tone="warning" icon="warning" message={installDisabledMessage} />
  {/if}

  <div class="flex flex-wrap items-center justify-end gap-2 px-1">
    {#if downloadProgress}
      <div class="mr-auto flex-1">
        {@render downloadProgress()}
      </div>
    {/if}

    {#if installBlocked}
      <Button type="button" size="sm" disabled>
        {t(labels.installAction)}
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

<AddonRiskConfirmDialog
  open={confirmOpen}
  {busy}
  {riskText}
  titleKey={labels.confirmTitle}
  bodyKey={labels.confirmBody}
  acceptKey={labels.confirmAccept}
  onOpenChange={setInstallConfirmOpen}
  onConfirm={installConfirmed}
/>
