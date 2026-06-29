<script lang="ts">
  import { untrack } from 'svelte';

  import { DownloadProgressBar } from '@entities/library';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import {
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Spinner,
  } from '@shared/ui';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import ExternalLinkIcon from '@lucide/svelte/icons/external-link';

  import { createRenoDxStore, type RenoDxStore } from '../model/create-renodx-store.svelte';
  import { humanizeMessageKey, riskFallbackKey } from '../model/reshade-presenters';
  import type { ReshadeChannel } from '../model/types';
  import RenoDxChannelSelect from './RenoDxChannelSelect.svelte';
  import RenoDxConfidenceBadge from './RenoDxConfidenceBadge.svelte';
  import RenoDxExternalInstall from './RenoDxExternalInstall.svelte';
  import RenoDxInstalledPanel from './RenoDxInstalledPanel.svelte';
  import RenoDxManualInstall from './RenoDxManualInstall.svelte';
  import RenoDxStateMessage from './RenoDxStateMessage.svelte';

  type Props = {
    gameId: string;
    /** Global busy flag (any exclusive operation in flight). */
    busy?: boolean;
    /**
     * Optional store, injected by the page so RenoDX folds into its "Update all"
     * action (single source of truth). When omitted the card self-manages its own
     * store and loads it, so it stays usable standalone.
     */
    store?: RenoDxStore;
  };

  const { gameId, busy = false, store: injectedStore }: Props = $props();

  // The store identity is fixed for the card's lifetime (a parent either always
  // injects the same store or never does), so capture it once.
  const store = untrack(() => injectedStore) ?? createRenoDxStore();
  // Whether the anti-cheat confirmation step is being shown for the warn case.
  let confirming = $state(false);

  // Reload whenever the selected game changes — but only when the card owns its
  // store; an injected store is loaded (and reloaded) by the page.
  $effect(() => {
    if (injectedStore) {
      return;
    }
    void store.load(gameId);
  });

  const combinedBusy = $derived(busy || store.busy);

  // The manual file-install escape hatch (a DirectX game with no automatic or
  // curated-external path), surfaced under the "unsupported"/"incompatible" states.
  const manualInstall = $derived(store.manualInstall);

  // A ReShade host conflict that would make a fresh install refuse: a non-ReShade
  // file occupying the proxy slot, ReShade in a slot the game won't load, or more
  // than one host present. Surfaced before the install button so the failure is
  // explained up front rather than only as an install error.
  const hostConflict = $derived(store.reshadeConflict || store.reshadeHostAction === 'conflict');
  const installsManagedHost = $derived(
    store.reshadeHostAction === 'update_host' ||
      store.reshadeHostAction === 'repair_host' ||
      store.reshadeHostAction === 'reinstall_with_addon_support',
  );

  // Risk text: prefer a backend-provided message key, falling back to a
  // severity-based localized string when the key is not in the catalog.
  const riskText = $derived(
    store.risk ? translateKey(store.risk.message_key, t(riskFallbackKey(store.risk.severity))) : '',
  );
  const installLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : t('gameDetails.renodx.actionInstall'),
  );

  // Inline notes/requirements (e.g. "run in DirectX"), each an i18n key with a
  // humanized fallback for keys not yet in the catalog.
  const notes = $derived(store.notesKeys.map((key) => translateKey(key, humanizeMessageKey(key))));

  // Localized incompatibility reason, falling back to the humanized enum name.
  const incompatibleReason = $derived(
    store.outcome?.kind === 'incompatible'
      ? translateKey(
          `gameDetails.renodx.reason.${store.outcome.reason.reason}`,
          store.outcome.reason.reason.replace(/_/g, ' '),
        )
      : '',
  );

  // External (Discord/Nexus) link label, from the manifest's i18n key.
  const externalLabel = $derived(
    store.externalLabelKey
      ? translateKey(store.externalLabelKey, t('gameDetails.renodx.actionOpenExternal'))
      : t('gameDetails.renodx.actionOpenExternal'),
  );

  // Blacklist explanation: a manifest-provided i18n key, else a generic line.
  const blacklistText = $derived(
    store.blacklistReason
      ? translateKey(store.blacklistReason, t('gameDetails.renodx.blacklisted'))
      : t('gameDetails.renodx.blacklisted'),
  );

  // An anti-cheat warning and/or the global-Vulkan-layer consent gate the install
  // behind an explicit confirmation step; a safe Direct3D title installs directly.
  const needsConfirm = $derived(store.requiresConfirmation || store.vulkanConsentNeeded);
  const confirmMessage = $derived.by(() => {
    const parts: string[] = [];
    if (store.requiresConfirmation) {
      parts.push(t('gameDetails.renodx.confirmBody'));
    }
    if (store.vulkanConsentNeeded) {
      parts.push(t('gameDetails.renodx.vulkanLayer.consentBody'));
    }
    return parts.join(' ');
  });
  const confirmAcceptLabel = $derived(
    store.requiresConfirmation
      ? t('gameDetails.renodx.confirmAccept')
      : t('gameDetails.renodx.vulkanLayer.consentAccept'),
  );

  // Single install entry point: a warn-risk or Vulkan-consent title steps through
  // confirmation, everything else installs directly. The confirmation passes the
  // anti-cheat and Vulkan-layer consents the backend gates require.
  function startInstall(): void {
    if (needsConfirm) {
      confirming = true;
    } else {
      void store.install(gameId, store.selectedReshadeChannel, false);
    }
  }

  function installConfirmed(): void {
    confirming = false;
    void store.install(gameId, store.selectedReshadeChannel, true, store.vulkanConsentNeeded);
  }

  function selectChannel(channel: ReshadeChannel): void {
    store.setSelectedReshadeChannel(channel);
  }

  function retry(): void {
    void store.load(gameId);
  }

  async function openExternalLink(): Promise<void> {
    if (store.externalUrl) {
      await openExternal(store.externalUrl);
    }
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <CardTitle>{t('gameDetails.renodx.title')}</CardTitle>
    <CardDescription>{t('gameDetails.renodx.description')}</CardDescription>
  </CardHeader>

  <CardContent class="flex w-full flex-col gap-4">
    {#if store.loading && !store.loaded}
      <Spinner class="size-4" />
      <span class="text-sm text-muted-foreground">{t('gameDetails.renodx.loading')}</span>
    {:else if store.loadError}
      <RenoDxStateMessage
        tone="warning"
        icon="warning"
        message={t('gameDetails.renodx.loadFailed')}
      >
        {#snippet actions()}
          <Button variant="outline" size="sm" disabled={combinedBusy} onclick={retry}>
            {t('gameDetails.renodx.retry')}
          </Button>
        {/snippet}
      </RenoDxStateMessage>
    {:else if store.isInstalled}
      <RenoDxInstalledPanel {gameId} {store} busy={combinedBusy} />
    {:else if store.isExternal}
      {#if store.externalFileInstallable}
        <RenoDxExternalInstall {gameId} {store} busy={combinedBusy} />
      {:else}
        <RenoDxStateMessage icon="info" message={t('gameDetails.renodx.external')}>
          {#snippet actions()}
            <Button variant="outline" size="sm" disabled={combinedBusy} onclick={openExternalLink}>
              <ExternalLinkIcon class="size-4" aria-hidden="true" />
              {externalLabel}
            </Button>
          {/snippet}
        </RenoDxStateMessage>
      {/if}
    {:else if store.isNativeHdr}
      <RenoDxStateMessage icon="hdr" message={t('gameDetails.renodx.nativeHdr')} />
    {:else if store.isBlacklisted}
      <RenoDxStateMessage tone="warning" icon="warning" message={blacklistText} />
    {:else if store.isUnsupported}
      <div class="flex w-full flex-col gap-3">
        <RenoDxStateMessage icon="unsupported" message={t('gameDetails.renodx.unsupported')} />
        {#if manualInstall}
          <RenoDxManualInstall {gameId} {store} manual={manualInstall} busy={combinedBusy} />
        {/if}
      </div>
    {:else if store.isIncompatible}
      <div class="flex w-full flex-col gap-3">
        <RenoDxStateMessage
          tone="warning"
          icon="warning"
          message={t('gameDetails.renodx.incompatible', { reason: incompatibleReason })}
        />
        {#if manualInstall}
          <RenoDxManualInstall {gameId} {store} manual={manualInstall} busy={combinedBusy} />
        {/if}
      </div>
    {:else if store.isInstallable}
      <div class="flex w-full flex-col gap-3">
        {#if store.confidence}
          <div>
            <RenoDxConfidenceBadge confidence={store.confidence} />
          </div>
        {/if}

        {#if hostConflict}
          <RenoDxStateMessage
            tone="warning"
            icon="warning"
            message={t('gameDetails.renodx.host.conflictBlocksInstall')}
          />
        {/if}

        {#if store.isBlocked}
          <RenoDxStateMessage tone="warning" icon="warning" message={riskText} />
        {:else if needsConfirm && confirming}
          <RenoDxStateMessage tone="warning" icon="warning" message={confirmMessage}>
            {#snippet actions()}
              <Button size="sm" variant="outline" onclick={() => (confirming = false)}>
                {t('gameDetails.renodx.cancel')}
              </Button>
              <Button
                size="sm"
                variant={store.requiresConfirmation ? 'destructive' : 'default'}
                disabled={combinedBusy}
                onclick={installConfirmed}
              >
                {confirmAcceptLabel}
              </Button>
            {/snippet}
          </RenoDxStateMessage>
        {:else if store.requiresConfirmation}
          <RenoDxStateMessage tone="warning" icon="warning" message={riskText} />
        {:else}
          <p class="text-sm text-muted-foreground">{riskText}</p>
        {/if}

        {#if notes.length > 0}
          <ul class="list-inside list-disc text-xs text-muted-foreground">
            {#each notes as note (note)}
              <li>{note}</li>
            {/each}
          </ul>
        {/if}

        {#if !(needsConfirm && confirming)}
          {#if installsManagedHost && !hostConflict}
            <RenoDxChannelSelect
              value={store.selectedReshadeChannel}
              stableSupported={store.reshadeStableSupported}
              disabled={combinedBusy}
              onValueChange={selectChannel}
            />
          {/if}
          <div class="flex items-center gap-2">
            <DownloadProgressBar ids={[gameId]} active={store.busy} />
            {#if store.isBlocked}
              <Button size="sm" disabled>{t('gameDetails.renodx.actionInstall')}</Button>
            {:else}
              <Button size="sm" disabled={combinedBusy} onclick={startInstall}>
                {#if store.busy}
                  <Spinner class="size-4" />
                {:else}
                  <DownloadIcon class="size-4" aria-hidden="true" />
                {/if}
                {installLabel}
              </Button>
            {/if}
          </div>
        {/if}
      </div>
    {:else}
      <RenoDxStateMessage icon="info" message={t('gameDetails.renodx.unavailable')} />
    {/if}
  </CardContent>
</Card>
