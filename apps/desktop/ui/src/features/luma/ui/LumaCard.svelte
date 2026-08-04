<script lang="ts">
  import { untrack } from 'svelte';

  import { t, translateExternalMessage } from '@shared/i18n';
  import { AddonCardShell, AddonStateMessage } from '@entities/addon';

  import { getCardView } from '../model/card-view';
  import { LUMA_ATTRIBUTION } from '../model/attribution';
  import { createLumaStore, type LumaStore } from '../model/create-luma-store.svelte';
  import LumaBlockedView from './LumaBlockedView.svelte';
  import LumaInstallableView from './LumaInstallableView.svelte';
  import LumaInstalledPanel from './LumaInstalledPanel.svelte';

  type Props = {
    gameId: string;
    /** Global busy flag: any exclusive page-level operation is in flight. */
    busy?: boolean;
    /** The game's launcher, for the launch-args callout's instructions. */
    launcher: string;
    /**
     * Optional externally owned store.
     *
     * When provided, the card does not auto-load it. User-triggered actions
     * still call methods on that store because the store remains the source of
     * truth for Luma operations.
     */
    store?: LumaStore;
  };

  const { gameId, busy: pageBusy = false, launcher, store: injectedStore }: Props = $props();

  /*
   * Store ownership is captured once on component creation.
   *
   * This prevents a prop change from silently switching ownership semantics:
   * - injected store => parent owns automatic loading
   * - no injected store => card owns automatic loading
   */
  const initialInjectedStore = untrack(() => injectedStore);
  const store = initialInjectedStore ?? createLumaStore();
  const ownsStore = initialInjectedStore === undefined;

  $effect(() => {
    if (!ownsStore) {
      return;
    }

    void store.load(gameId);
  });

  const combinedBusy = $derived(pageBusy || store.busy);
  const view = $derived.by(() => getCardView(store));
  const progressIds = $derived([gameId]);
  const showLoading = $derived(view === 'loading');
  const showLoadError = $derived(view === 'load-error');
  const showAttribution = $derived(view !== 'installable' && view !== 'installed');

  const blacklistText = $derived(
    store.blacklistMessage
      ? translateExternalMessage({
          key: store.blacklistMessage.id,
          fallback: store.blacklistMessage.fallback_text,
        })
      : t('gameDetails.luma.blacklisted'),
  );

  const incompatibleReason = $derived.by((): string => {
    if (store.outcome?.kind !== 'incompatible') {
      return '';
    }

    const reason = store.outcome.reason.reason;

    return translateExternalMessage({
      key: `gameDetails.luma.reason.${reason}`,
      fallback: reason.replaceAll('_', ' '),
    });
  });

  function retry(): void {
    if (combinedBusy) {
      return;
    }

    void store.retry(gameId);
  }
</script>

<AddonCardShell
  title={t('gameDetails.luma.title')}
  description={t('gameDetails.luma.description')}
  loadingLabel={t('gameDetails.luma.loading')}
  {progressIds}
  progressActive={store.busy}
  actionsDisabled={combinedBusy}
  {showLoading}
  {showLoadError}
  retrying={store.loading}
  {showAttribution}
  attribution={LUMA_ATTRIBUTION}
  onRetry={retry}
>
  {#if view === 'installed'}
    <LumaInstalledPanel {gameId} {store} busy={combinedBusy} {launcher} />
  {:else if view === 'blocked-by-other-addon' || view === 'unmanaged-present'}
    <LumaBlockedView {store} />
  {:else if view === 'blacklisted'}
    <AddonStateMessage tone="warning" icon="warning" message={blacklistText} />
  {:else if view === 'unsupported'}
    <AddonStateMessage icon="unsupported" message={t('gameDetails.luma.unsupported')} />
  {:else if view === 'incompatible'}
    <AddonStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.luma.incompatible', { reason: incompatibleReason })}
    />
  {:else if view === 'installable'}
    <LumaInstallableView {gameId} {store} busy={combinedBusy} {launcher} />
  {:else}
    <AddonStateMessage icon="info" message={t('gameDetails.luma.unavailable')} />
  {/if}
</AddonCardShell>
