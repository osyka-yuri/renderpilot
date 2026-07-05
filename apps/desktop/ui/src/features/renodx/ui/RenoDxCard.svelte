<script lang="ts">
  import { untrack } from 'svelte';

  import { t, translateKey } from '@shared/i18n';
  import { ADDON_DISPLAY_NAME } from '@shared/model';
  import {
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import SettingsIcon from '@lucide/svelte/icons/settings';

  import { getCardView } from '../model/card-view';
  import { createRenoDxStore, type RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxExternalView from './RenoDxExternalView.svelte';
  import RenoDxInstallableView from './RenoDxInstallableView.svelte';
  import RenoDxInstalledPanel from './RenoDxInstalledPanel.svelte';
  import RenoDxManualFallbackView from './RenoDxManualFallbackView.svelte';
  import { AddonStateMessage } from '@entities/addon';

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
  };

  const {
    gameId,
    busy: pageBusy = false,
    store: injectedStore,
    onOpenRenoDxSettings,
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

  const blacklistText = $derived(
    store.blacklistReason
      ? translateKey(store.blacklistReason, t('gameDetails.renodx.blacklisted'))
      : t('gameDetails.renodx.blacklisted'),
  );

  const blockedByOtherAddonText = $derived(
    t(
      store.otherAddonUnmanaged
        ? 'gameDetails.addon.blockedByOtherAddon.unmanaged'
        : 'gameDetails.addon.blockedByOtherAddon.tracked',
      {
        installedAddon: store.otherAddonKind ? ADDON_DISPLAY_NAME[store.otherAddonKind] : 'add-on',
        blockedAddon: ADDON_DISPLAY_NAME.renodx,
      },
    ),
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

    void store.load(gameId);
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0 space-y-1.5">
        <CardTitle>{t('gameDetails.renodx.title')}</CardTitle>
        <CardDescription>{t('gameDetails.renodx.description')}</CardDescription>
      </div>

      {#if showVulkanSettingsAction}
        <Tooltip>
          <TooltipTrigger
            type="button"
            aria-label={vulkanSettingsLabel}
            onclick={onOpenRenoDxSettings}
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
    </div>
  </CardHeader>

  <CardContent class="flex w-full flex-col gap-4">
    {#if view === 'loading'}
      <div class="flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner class="size-4" />
        <span>{t('gameDetails.renodx.loading')}</span>
      </div>
    {:else if view === 'load-error'}
      <AddonStateMessage tone="warning" icon="warning" message={t('gameDetails.renodx.loadFailed')}>
        {#snippet actions()}
          <Button type="button" variant="outline" size="sm" disabled={combinedBusy} onclick={retry}>
            {t('gameDetails.renodx.retry')}
          </Button>
        {/snippet}
      </AddonStateMessage>
    {:else if view === 'installed'}
      <RenoDxInstalledPanel {gameId} {store} busy={combinedBusy} />
    {:else if view === 'blocked-by-other-addon'}
      <AddonStateMessage tone="default" icon="info" message={blockedByOtherAddonText} />
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
  </CardContent>
</Card>
