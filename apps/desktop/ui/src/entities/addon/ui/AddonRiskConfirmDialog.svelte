<script lang="ts">
  import DownloadIcon from '@lucide/svelte/icons/download';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from '@shared/ui';

  type Props = {
    open: boolean;
    busy: boolean;
    riskText: string;
    titleKey: MessageKeyWithoutParams;
    bodyKey: MessageKeyWithoutParams;
    acceptKey: MessageKeyWithoutParams;
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
  };

  const { open, busy, riskText, titleKey, bodyKey, acceptKey, onOpenChange, onConfirm }: Props =
    $props();

  const riskMessage = $derived(riskText.trim());

  function requestOpenChange(nextOpen: boolean): void {
    if (busy && !nextOpen) {
      return;
    }

    onOpenChange(nextOpen);
  }

  function cancel(): void {
    requestOpenChange(false);
  }

  function confirm(): void {
    if (busy) {
      return;
    }

    onConfirm();
  }
</script>

<Dialog {open} onOpenChange={requestOpenChange}>
  <DialogContent closeLabel={t('common.close')} class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>{t(titleKey)}</DialogTitle>
      <DialogDescription>
        {t(bodyKey)}
      </DialogDescription>
    </DialogHeader>

    {#if riskMessage}
      <div
        role="alert"
        class="flex gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"
      >
        <TriangleAlertIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
        <p class="min-w-0">{riskMessage}</p>
      </div>
    {/if}

    <DialogFooter>
      <Button variant="secondary" size="sm" disabled={busy} onclick={cancel}>
        {t('common.cancel')}
      </Button>

      <Button variant="destructive" size="sm" disabled={busy} onclick={confirm}>
        <DownloadIcon class="size-4" aria-hidden="true" />
        {t(acceptKey)}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
