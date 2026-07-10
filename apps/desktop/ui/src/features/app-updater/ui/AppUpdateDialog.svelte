<script lang="ts">
  import { getLocale, t } from '@shared/i18n';
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    ScrollArea,
  } from '@shared/ui';

  import { canDismissDialog, dialogFooter, failureKind, progressPhase } from '../model/dialog-view';
  import { formatReleaseDateForLocale } from '../model/format-release-date';
  import type { AppUpdateDialogState } from '../model/types';
  import UpdateChangelog from './UpdateChangelog.svelte';
  import UpdateDialogActions from './UpdateDialogActions.svelte';
  import UpdateFailureAlert from './UpdateFailureAlert.svelte';
  import UpdateProgress from './UpdateProgress.svelte';

  type Props = {
    state: AppUpdateDialogState | null;
    onInstall: () => void;
    onRetry: () => void;
    onDismiss: () => void;
    onRestart: () => void;
  };

  const { state, onInstall, onRetry, onDismiss, onRestart }: Props = $props();

  const open = $derived(state !== null);
  const canDismiss = $derived(canDismissDialog(state));
  const offer = $derived(state?.offer ?? null);
  const phase = $derived(progressPhase(state));
  const failure = $derived(failureKind(state));
  const footer = $derived(dialogFooter(state));

  const versionLine = $derived(
    offer
      ? t('settings.about.updateDialog.versionLine', {
          currentVersion: offer.currentVersion,
          version: offer.version,
        })
      : '',
  );

  const releaseDateLabel = $derived(
    offer ? formatReleaseDateForLocale(offer.date, getLocale()) : null,
  );

  const progress = $derived(
    state?.phase === 'downloading' || state?.phase === 'verifying' ? state.progress : null,
  );
</script>

<Dialog
  {open}
  onOpenChange={(nextOpen) => {
    if (!nextOpen && canDismiss) {
      onDismiss();
    }
  }}
>
  <DialogContent
    class="sm:max-w-lg"
    showCloseButton={canDismiss}
    escapeKeydownBehavior={canDismiss ? 'close' : 'ignore'}
    interactOutsideBehavior={canDismiss ? 'close' : 'ignore'}
  >
    <DialogHeader>
      <DialogTitle>{t('settings.about.updateDialog.title')}</DialogTitle>
      {#if offer}
        <DialogDescription class="flex flex-col gap-1">
          <span>{versionLine}</span>
          {#if releaseDateLabel}
            <span class="text-xs text-muted-foreground">
              {t('settings.about.updateDialog.releaseDate', { date: releaseDateLabel })}
            </span>
          {/if}
        </DialogDescription>
      {/if}
    </DialogHeader>

    <div class="flex flex-col gap-4">
      {#if offer && state?.phase !== 'restarting'}
        <div class="flex flex-col gap-2">
          <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
            {t('settings.about.updateDialog.releaseNotes')}
          </p>
          <ScrollArea class="h-[min(45vh,28rem)] rounded-md border">
            <div class="p-3">
              <UpdateChangelog document={offer.releaseNotes} />
            </div>
          </ScrollArea>
        </div>
      {/if}

      {#if phase}
        <UpdateProgress {phase} {progress} />
      {/if}

      {#if failure}
        <UpdateFailureAlert kind={failure} />
      {/if}
    </div>

    <DialogFooter>
      <UpdateDialogActions {footer} {onInstall} {onRetry} {onDismiss} {onRestart} />
    </DialogFooter>
  </DialogContent>
</Dialog>
