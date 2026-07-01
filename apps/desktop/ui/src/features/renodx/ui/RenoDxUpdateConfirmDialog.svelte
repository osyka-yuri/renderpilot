<script lang="ts">
  import { t } from '@shared/i18n';
  import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
  } from '@shared/ui';
  import DownloadIcon from '@lucide/svelte/icons/download';

  type Props = {
    open?: boolean;
    busy?: boolean;
    onConfirm: () => unknown;
  };

  let { open = $bindable(), busy = false, onConfirm }: Props = $props();

  let confirming = $state(false);

  const disabled = $derived(busy || confirming);

  async function confirmUpdate(): Promise<void> {
    if (disabled) {
      return;
    }

    confirming = true;

    try {
      await onConfirm();
      open = false;
    } finally {
      confirming = false;
    }
  }

  function handleOpenChange(nextOpen: boolean): void {
    if (disabled && !nextOpen) {
      return;
    }
    open = nextOpen;
  }
</script>

<AlertDialog {open} onOpenChange={handleOpenChange}>
  <AlertDialogContent class="sm:max-w-md" escapeKeydownBehavior={disabled ? 'ignore' : 'close'}>
    <AlertDialogHeader>
      <AlertDialogTitle>{t('gameDetails.renodx.updateConfirmTitle')}</AlertDialogTitle>
      <AlertDialogDescription>
        {t('gameDetails.renodx.updateConfirmBody')}
      </AlertDialogDescription>
    </AlertDialogHeader>

    <AlertDialogFooter>
      <AlertDialogCancel type="button" {disabled}>
        {t('common.cancel')}
      </AlertDialogCancel>

      <AlertDialogAction type="button" {disabled} onclick={confirmUpdate}>
        <DownloadIcon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.updateConfirmAction')}
      </AlertDialogAction>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
