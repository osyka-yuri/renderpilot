<script lang="ts">
  import { onMount } from 'svelte';
  import AlertTriangleIcon from '@lucide/svelte/icons/alert-triangle';
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import WrenchIcon from '@lucide/svelte/icons/wrench';

  import { DownloadProgressBar } from '@entities/library';
  import {
    createVulkanLayerSettingsStore,
    RenoDxChannelControl,
    RenoDxStatusBadge,
    RenoDxFieldLabel,
    RenoDxComponentRow,
    VULKAN_LAYER_PROGRESS_ID,
    VULKAN_DIAGNOSTIC_LABEL,
    VULKAN_LAYER_PRIMARY_ACTION_LABEL,
    VULKAN_LAYER_STATE_LABEL,
    VULKAN_LOADER_VISIBILITY_NOTE,
    hostVersionDescription,
  } from '@features/renodx';
  import type { ReshadeChannel } from '@features/renodx';
  import { t } from '@shared/i18n';
  import type { MessageKey } from '@shared/i18n';
  import {
    Badge,
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    AlertDialog,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
    Item,
    ItemContent,
    ItemGroup,
    Spinner,
  } from '@shared/ui';

  import SettingsStatusMessage from './SettingsStatusMessage.svelte';

  const store = createVulkanLayerSettingsStore();

  type PrimaryAction = NonNullable<typeof store.primaryAction>;
  type VisiblePrimaryAction = Exclude<PrimaryAction, 'switch_channel'>;

  let removeConfirmOpen = $state(false);

  onMount(() => {
    loadLayer();
  });

  const layer = $derived(store.layer);
  const actions = $derived(layer?.actions);
  const facts = $derived(layer?.layer_facts ?? null);
  const detection = $derived(layer?.layer_detection ?? null);
  const reasons = $derived(layer?.diagnostic_reasons ?? []);

  const isInitialLoading = $derived(store.loading && !store.report);
  const controlsDisabled = $derived(store.busy || store.loading);
  const isInstalled = $derived(detection === 'installed');
  const displayState = $derived(store.displayState);

  const loaderVisibilityNote = $derived<MessageKey | null>(
    facts && facts.loader_visibility !== 'normal'
      ? VULKAN_LOADER_VISIBILITY_NOTE[facts.loader_visibility]
      : null,
  );

  const showDiagnostics = $derived(
    Boolean(loaderVisibilityNote) || detection === 'external_read_only' || reasons.length > 0,
  );

  const showReasonIcon = $derived(
    detection !== 'external_read_only' && loaderVisibilityNote === null,
  );

  const visiblePrimaryAction = $derived<VisiblePrimaryAction | null>(
    store.primaryAction && store.primaryAction !== 'switch_channel' ? store.primaryAction : null,
  );

  const primaryActionLabel = $derived<MessageKey | null>(
    visiblePrimaryAction ? VULKAN_LAYER_PRIMARY_ACTION_LABEL[visiblePrimaryAction] : null,
  );

  const primaryActionDisabled = $derived(
    controlsDisabled || !store.primaryActionDescriptor?.enabled,
  );

  const primaryActionVariant = $derived(visiblePrimaryAction === 'repair' ? 'outline' : 'default');

  const removeAction = $derived(actions?.remove);
  const showRemove = $derived(Boolean(removeAction));
  const removeDisabled = $derived(store.busy || !removeAction?.enabled);

  const reshadeDescription = $derived.by(() => {
    if (!isInstalled) {
      return isInitialLoading ? '\u00A0' : '';
    }

    const description = hostVersionDescription(facts?.version);
    return description.kind === 'version'
      ? t(description.key, { version: description.version })
      : t(description.key);
  });

  async function setChannel(channel: ReshadeChannel): Promise<void> {
    await store.setSelectedChannel(channel);
    await store.apply();
  }

  function loadLayer(): void {
    void store.load();
  }

  function applyLayer(): void {
    void store.apply();
  }

  function setRemoveConfirmOpen(open: boolean): void {
    removeConfirmOpen = open;
  }

  function openRemoveConfirm(): void {
    removeConfirmOpen = true;
  }

  function closeRemoveConfirm(): void {
    removeConfirmOpen = false;
  }

  function confirmRemoveLayer(): void {
    removeConfirmOpen = false;
    void store.remove();
  }
</script>

<Card>
  <CardHeader>
    <CardTitle>{t('gameDetails.renodx.vulkanLayer.title')}</CardTitle>
    <CardDescription>{t('settings.renodx.vulkan.description')}</CardDescription>
  </CardHeader>

  <CardContent class="flex flex-col gap-6">
    {#if store.error}
      <SettingsStatusMessage message={store.error} kind="error" />
    {/if}

    <div class="flex flex-col gap-4">
      <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
        <RenoDxFieldLabel label={t('gameDetails.renodx.status.label')} class="flex-nowrap gap-1.5">
          {#if displayState}
            <Badge variant={displayState === 'installed' ? 'secondary' : 'outline'}>
              {t(VULKAN_LAYER_STATE_LABEL[displayState])}
            </Badge>
          {/if}
        </RenoDxFieldLabel>

        {#if facts && isInstalled && store.updateStatus}
          <RenoDxFieldLabel label={t('gameDetails.renodx.fresh.label')} class="flex-nowrap gap-1.5">
            <RenoDxStatusBadge status={store.updateStatus} />
          </RenoDxFieldLabel>
        {/if}
      </div>

      <ItemGroup class="rounded-md border bg-muted/30">
        <RenoDxComponentRow
          icon="reshade"
          title={t('gameDetails.renodx.component.reshade')}
          description={reshadeDescription}
        >
          {#snippet actions()}
            <RenoDxChannelControl
              value={store.selectedChannel}
              stableSupported={store.stableSupported}
              busy={controlsDisabled}
              ariaLabel={t('settings.renodx.vulkan.channel')}
              title={t('settings.renodx.vulkan.channelDescription')}
              onChange={setChannel}
            />
          {/snippet}
        </RenoDxComponentRow>

        {#if showDiagnostics}
          <Item>
            <ItemContent class="text-amber-600 dark:text-amber-500">
              {#if loaderVisibilityNote}
                <div class="mt-1 flex items-start gap-2 text-sm font-medium">
                  <AlertTriangleIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <p>{t(loaderVisibilityNote)}</p>
                </div>
              {/if}

              {#if detection === 'external_read_only'}
                <div class="mt-1 flex items-start gap-2 text-sm font-medium">
                  <AlertTriangleIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <p>{t('gameDetails.renodx.vulkanLayer.externalReadOnly')}</p>
                </div>
              {/if}

              {#if reasons.length > 0}
                <div class="mt-1 flex items-start gap-2 text-sm font-medium">
                  {#if showReasonIcon}
                    <AlertTriangleIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  {/if}

                  <ul class="list-inside list-disc">
                    {#each reasons as reason (reason)}
                      <li>{t(VULKAN_DIAGNOSTIC_LABEL[reason])}</li>
                    {/each}
                  </ul>
                </div>
              {/if}
            </ItemContent>
          </Item>
        {/if}
      </ItemGroup>

      <div class="flex flex-wrap items-center gap-2">
        <DownloadProgressBar ids={[VULKAN_LAYER_PROGRESS_ID]} active={store.busy} />

        <Button variant="outline" size="sm" disabled={controlsDisabled} onclick={loadLayer}>
          {#if store.loading}
            <Spinner class="size-4" />
            {t('gameDetails.renodx.fresh.checking')}
          {:else}
            <RotateCwIcon class="size-4" aria-hidden="true" />
            {t('gameDetails.renodx.actionCheckUpdates')}
          {/if}
        </Button>

        {#if visiblePrimaryAction && primaryActionLabel}
          <Button
            variant={primaryActionVariant}
            size="sm"
            disabled={primaryActionDisabled}
            onclick={applyLayer}
          >
            {#if store.busy}
              <Spinner class="size-4" />
            {:else if visiblePrimaryAction === 'install'}
              <DownloadIcon class="size-4" aria-hidden="true" />
            {:else if visiblePrimaryAction === 'repair'}
              <WrenchIcon class="size-4" aria-hidden="true" />
            {:else if visiblePrimaryAction === 'update'}
              <CircleArrowUpIcon class="size-4" aria-hidden="true" />
            {:else}
              <RotateCwIcon class="size-4" aria-hidden="true" />
            {/if}

            {t(primaryActionLabel)}
          </Button>
        {/if}

        {#if showRemove}
          <Button
            variant="destructive"
            size="sm"
            class="ml-auto"
            disabled={removeDisabled}
            onclick={openRemoveConfirm}
            title={t('gameDetails.renodx.vulkanLayer.action.remove')}
          >
            <Trash2Icon class="size-4" aria-hidden="true" />
            {t('gameDetails.renodx.vulkanLayer.action.remove')}
          </Button>
        {/if}
      </div>
    </div>
  </CardContent>
</Card>

<AlertDialog open={removeConfirmOpen} onOpenChange={setRemoveConfirmOpen}>
  <AlertDialogContent class="sm:max-w-md">
    <AlertDialogHeader>
      <AlertDialogTitle>{t('gameDetails.renodx.vulkanLayer.removeConfirmTitle')}</AlertDialogTitle>
      <AlertDialogDescription>
        {t('gameDetails.renodx.vulkanLayer.removeConfirmBody')}
      </AlertDialogDescription>
    </AlertDialogHeader>

    <AlertDialogFooter>
      <Button variant="secondary" size="sm" onclick={closeRemoveConfirm}>
        {t('common.cancel')}
      </Button>

      <Button variant="destructive" size="sm" disabled={store.busy} onclick={confirmRemoveLayer}>
        <Trash2Icon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.vulkanLayer.action.remove')}
      </Button>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
