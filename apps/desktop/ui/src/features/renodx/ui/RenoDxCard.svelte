<script lang="ts">
  import { untrack } from 'svelte';

  import { DownloadProgressBar } from '@entities/library';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import {
    Badge,
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
  import type { MatchConfidence } from '../model/types';
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

  // Confidence badge: an honest label for how the match was made. `verified`
  // reads as a positive (secondary) badge; the weaker tiers stay outlined.
  const CONFIDENCE_LABELS = {
    verified: 'gameDetails.renodx.confidenceVerified',
    experimental: 'gameDetails.renodx.confidenceExperimental',
    untested: 'gameDetails.renodx.confidenceUntested',
  } as const satisfies Record<MatchConfidence, string>;
  const confidenceVariant = (confidence: MatchConfidence | null): 'secondary' | 'outline' =>
    confidence === 'verified' ? 'secondary' : 'outline';

  // Risk text: prefer a backend-provided message key, falling back to a
  // severity-based localized string when the key is not in the catalog.
  const riskFallback = $derived(
    store.isBlocked
      ? t('gameDetails.renodx.riskBlocked')
      : store.requiresConfirmation
        ? t('gameDetails.renodx.riskWarn')
        : t('gameDetails.renodx.riskSafe'),
  );
  const riskText = $derived(store.risk ? translateKey(store.risk.message_key, riskFallback) : '');
  const confidenceLabel = $derived(store.confidence ? t(CONFIDENCE_LABELS[store.confidence]) : '');

  const installLabel = $derived(
    store.busy ? t('gameDetails.renodx.installing') : t('gameDetails.renodx.actionInstall'),
  );

  // Inline notes/requirements (e.g. "run in DirectX"), each an i18n key with a
  // humanized fallback for keys not yet in the catalog.
  const humanizeKey = (key: string): string => key.replace(/^.*\./, '').replace(/_/g, ' ');
  const notes = $derived(store.notesKeys.map((key) => translateKey(key, humanizeKey(key))));

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

  // Single install entry point: warn-risk titles step through confirmation,
  // everything else installs directly.
  function startInstall(): void {
    if (store.requiresConfirmation) {
      confirming = true;
    } else {
      void store.install(gameId, false);
    }
  }

  function installConfirmed(): void {
    confirming = false;
    void store.install(gameId, true);
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

  <CardContent class="flex flex-wrap items-center gap-3">
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
            <Badge variant={confidenceVariant(store.confidence)}>{confidenceLabel}</Badge>
          </div>
        {/if}

        {#if store.isBlocked}
          <RenoDxStateMessage tone="warning" icon="warning" message={riskText} />
        {:else if store.requiresConfirmation && confirming}
          <RenoDxStateMessage
            tone="warning"
            icon="warning"
            message={t('gameDetails.renodx.confirmBody')}
          >
            {#snippet actions()}
              <Button size="sm" variant="outline" onclick={() => (confirming = false)}>
                {t('gameDetails.renodx.cancel')}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                disabled={combinedBusy}
                onclick={installConfirmed}
              >
                {t('gameDetails.renodx.confirmAccept')}
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

        {#if !(store.requiresConfirmation && confirming)}
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
