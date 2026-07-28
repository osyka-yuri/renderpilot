<script lang="ts">
  import { untrack } from 'svelte';

  import { t, translateKey } from '@shared/i18n';
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import SettingsIcon from '@lucide/svelte/icons/settings';

  import { AddonBlockedMessage, AddonCardShell, AddonStateMessage } from '@entities/addon';

  import { getCardView } from '../model/card-view';
  import { createRenoDxStore, type RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxExternalView from './RenoDxExternalView.svelte';
  import RenoDxInstallableView from './RenoDxInstallableView.svelte';
  import RenoDxInstalledPanel from './RenoDxInstalledPanel.svelte';
  import RenoDxManualFallbackView from './RenoDxManualFallbackView.svelte';
  import { RENODX_ATTRIBUTION } from '../model/attribution';

  type Props = {
    gameId: string;
    /** Global busy flag: any exclusive page-level operation is in flight. */
    busy?: boolean;
    /**
     * Optional externally owned store.
     *
     * When provided, the card does not auto-load it. User-triggered actions
     * still call methods on that store because the store remains the source of
     * truth for RenoDX operations.
     */
    store?: RenoDxStore;
    onOpenRenoDxSettings: () => void;
    onPreloadRenoDxSettings?: () => void;
  };

  const {
    gameId,
    busy: pageBusy = false,
    store: injectedStore,
    onOpenRenoDxSettings,
    onPreloadRenoDxSettings = () => undefined,
  }: Props = $props();

  /*
   * Store ownership is captured once on component creation.
   *
   * This prevents a prop change from silently switching ownership semantics:
   * - injected store => parent owns automatic loading
   * - no injected store => card owns automatic loading
   */
  const initialInjectedStore = untrack(() => injectedStore);
  const store = initialInjectedStore ?? createRenoDxStore();
  const ownsStore = initialInjectedStore === undefined;

  $effect(() => {
    if (!ownsStore) {
      return;
    }

    void store.load(gameId);
  });

  const combinedBusy = $derived(pageBusy || store.busy);
  const view = $derived.by(() => getCardView(store));
  const manualInstall = $derived(store.manualInstall);
  const progressIds = $derived([gameId]);
  const showLoading = $derived(view === 'loading');
  const showLoadError = $derived(view === 'load-error');
  const showAttribution = $derived(
    view !== 'installable' && view !== 'installed' && view !== 'external',
  );

  const blacklistText = $derived(
    store.blacklistMessage
      ? translateKey(store.blacklistMessage.id, store.blacklistMessage.fallback_text)
      : t('gameDetails.renodx.blacklisted'),
  );

  const showVulkanSettingsAction = $derived.by((): boolean => {
    if (store.state?.status === 'installed') {
      return store.state.host_kind === 'vulkan';
    }

    switch (store.outcome?.kind) {
      case 'installable':
        return store.outcome.host_kind === 'vulkan';

      case 'external':
        return store.outcome.file_install?.host_kind === 'vulkan';

      default:
        return manualInstall?.host_kind === 'vulkan';
    }
  });

  const vulkanSettingsLabel = $derived(t('gameDetails.renodx.vulkanLayer.openSettings'));

  function retry(): void {
    if (combinedBusy) {
      return;
    }

    void store.retry(gameId);
  }
</script>

<AddonCardShell
  title={t('gameDetails.renodx.title')}
  description={t('gameDetails.renodx.description')}
  loadingLabel={t('gameDetails.renodx.loading')}
  {progressIds}
  progressActive={store.busy}
  actionsDisabled={combinedBusy}
  {showLoading}
  {showLoadError}
  retrying={store.loading}
  {showAttribution}
  attribution={RENODX_ATTRIBUTION}
  onRetry={retry}
>
  {#snippet headerActions()}
    {#if showVulkanSettingsAction}
      <Tooltip>
        <TooltipTrigger
          type="button"
          aria-label={vulkanSettingsLabel}
          onclick={onOpenRenoDxSettings}
          onpointerenter={onPreloadRenoDxSettings}
          onfocus={onPreloadRenoDxSettings}
        >
          {#snippet child({ props })}
            <Button {...props} variant="ghost" size="icon-sm">
              <SettingsIcon class="size-4" aria-hidden="true" />
            </Button>
          {/snippet}
        </TooltipTrigger>

        <TooltipContent>
          {vulkanSettingsLabel}
        </TooltipContent>
      </Tooltip>
    {/if}
  {/snippet}

  {#if view === 'installed'}
    <RenoDxInstalledPanel {gameId} {store} busy={combinedBusy} />
  {:else if view === 'blocked-by-other-addon'}
    <AddonBlockedMessage
      blockedAddon="renodx"
      installedAddon={store.otherAddonKind}
      fallbackInstalledAddon="luma"
      unmanaged={store.otherAddonUnmanaged}
    />
  {:else if view === 'external'}
    <RenoDxExternalView {gameId} {store} busy={combinedBusy} />
  {:else if view === 'native-hdr'}
    <AddonStateMessage icon="hdr" message={t('gameDetails.renodx.nativeHdr')} />
  {:else if view === 'blacklisted'}
    <AddonStateMessage tone="warning" icon="warning" message={blacklistText} />
  {:else if view === 'unsupported'}
    <RenoDxManualFallbackView {gameId} {store} busy={combinedBusy} variant="unsupported" />
  {:else if view === 'incompatible'}
    <RenoDxManualFallbackView {gameId} {store} busy={combinedBusy} variant="incompatible" />
  {:else if view === 'installable'}
    <RenoDxInstallableView {gameId} {store} busy={combinedBusy} />
  {:else}
    <AddonStateMessage icon="info" message={t('gameDetails.renodx.unavailable')} />
  {/if}
</AddonCardShell>
