<script lang="ts">
  import { onDestroy } from 'svelte';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import InfoIcon from '@lucide/svelte/icons/info';
  import {
    Alert,
    AlertDescription,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
  } from '@shared/ui';
  import { isDesktopPreviewMode } from '@shared/api-preview';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import {
    DEVELOPER_MODE_CHECK_UNAVAILABLE,
    type DeveloperModePlanBlocker,
  } from '../model/d3d12-preflight';
  import { openDeveloperModeSettings } from '../model/developer-mode-links';

  type Props = {
    open: boolean;
    blocker: DeveloperModePlanBlocker | null;
    retrying: boolean;
    stillDisabledAfterRetry: boolean;
    onOpenChange: (open: boolean) => void;
    onRetry: () => void;
  };

  const { open, blocker, retrying, stillDisabledAfterRetry, onOpenChange, onRetry }: Props =
    $props();

  type DialogCopy = {
    title: MessageKeyWithoutParams;
    description: MessageKeyWithoutParams;
    guidance: MessageKeyWithoutParams | null;
    openAction: MessageKeyWithoutParams | null;
    openFailed: MessageKeyWithoutParams | null;
    retryAction: MessageKeyWithoutParams;
  };

  let openingExternalTarget = $state(false);
  let externalTargetError = $state(false);
  let externalTargetGeneration = 0;
  const unavailable = $derived(blocker === DEVELOPER_MODE_CHECK_UNAVAILABLE);
  const actionsBusy = $derived(retrying || openingExternalTarget);
  const previewMode = isDesktopPreviewMode();
  const copy = $derived.by((): DialogCopy =>
    unavailable
      ? {
          title: 'gameDetails.developerMode.checkTitle',
          description: 'gameDetails.developerMode.checkDescription',
          guidance: null,
          openAction: null,
          openFailed: null,
          retryAction: 'gameDetails.developerMode.retryCheck',
        }
      : {
          title: 'gameDetails.developerMode.requiredTitle',
          description: 'gameDetails.developerMode.requiredDescription',
          guidance: previewMode
            ? 'gameDetails.developerMode.previewGuidance'
            : 'gameDetails.developerMode.enableGuidance',
          openAction: previewMode
            ? 'gameDetails.developerMode.openDocumentation'
            : 'gameDetails.developerMode.openSettings',
          openFailed: previewMode
            ? 'gameDetails.developerMode.documentationOpenFailed'
            : 'gameDetails.developerMode.settingsOpenFailed',
          retryAction: 'gameDetails.developerMode.checkStatus',
        },
  );

  $effect(() => {
    if (!open) {
      externalTargetGeneration++;
      openingExternalTarget = false;
      externalTargetError = false;
    }
  });

  onDestroy(() => {
    externalTargetGeneration++;
  });

  async function openExternalTarget(): Promise<void> {
    if (retrying || openingExternalTarget) {
      return;
    }
    const generation = ++externalTargetGeneration;
    openingExternalTarget = true;
    externalTargetError = false;
    try {
      await openDeveloperModeSettings();
      if (generation !== externalTargetGeneration) {
        return;
      }
    } catch {
      if (generation !== externalTargetGeneration) {
        return;
      }
      externalTargetError = true;
    } finally {
      if (generation === externalTargetGeneration) {
        openingExternalTarget = false;
      }
    }
  }

  function retryStatus(): void {
    if (retrying || openingExternalTarget) {
      return;
    }
    externalTargetError = false;
    onRetry();
  }

  function requestOpenChange(next: boolean): void {
    onOpenChange(next);
  }
</script>

<Dialog {open} onOpenChange={requestOpenChange}>
  <DialogContent closeLabel={t('common.close')}>
    <DialogHeader>
      <DialogTitle>{t(copy.title)}</DialogTitle>
      <DialogDescription>{t(copy.description)}</DialogDescription>
    </DialogHeader>

    {#if unavailable}
      <Alert variant="destructive" size="sm" role="alert">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{t('gameDetails.developerMode.checkUnavailable')}</AlertDescription>
      </Alert>
    {:else}
      <div class="grid gap-3">
        {#if copy.guidance}
          <p class="text-sm">{t(copy.guidance)}</p>
        {/if}
        <Alert size="sm" role="note">
          <InfoIcon aria-hidden="true" />
          <AlertDescription>{t('gameDetails.developerMode.restartInfo')}</AlertDescription>
        </Alert>
      </div>
    {/if}

    {#if stillDisabledAfterRetry && !unavailable}
      <Alert variant="warning" size="sm" role="status">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{t('gameDetails.developerMode.stillDisabled')}</AlertDescription>
      </Alert>
    {/if}
    {#if externalTargetError && copy.openFailed}
      <Alert variant="destructive" size="sm" role="alert">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{t(copy.openFailed)}</AlertDescription>
      </Alert>
    {/if}

    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        onclick={() => {
          requestOpenChange(false);
        }}
      >
        {t('common.cancel')}
      </Button>
      {#if !unavailable}
        <Button variant="outline" size="sm" disabled={actionsBusy} onclick={retryStatus}>
          {#if retrying}
            <Spinner aria-hidden="true" />
          {/if}
          {retrying ? t('gameDetails.developerMode.checkingStatus') : t(copy.retryAction)}
        </Button>
        <Button size="sm" disabled={actionsBusy} onclick={openExternalTarget}>
          {#if openingExternalTarget}
            <Spinner aria-hidden="true" />
          {/if}
          {#if copy.openAction}
            {t(copy.openAction)}
          {/if}
        </Button>
      {:else}
        <Button size="sm" disabled={actionsBusy} onclick={retryStatus}>
          {#if retrying}
            <Spinner aria-hidden="true" />
          {/if}
          {retrying ? t('gameDetails.developerMode.checkingStatus') : t(copy.retryAction)}
        </Button>
      {/if}
    </DialogFooter>
  </DialogContent>
</Dialog>
