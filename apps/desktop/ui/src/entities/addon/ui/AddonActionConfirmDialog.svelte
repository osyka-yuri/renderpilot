<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { t } from '@shared/i18n';
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
  } from '@shared/ui';

  type AddonActionTone = 'default' | 'warning' | 'destructive';

  type Props = {
    open: boolean;
    busy: boolean;
    tone: AddonActionTone;
    title: string;
    description: string;
    warning?: string;
    confirmLabel: string;
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
  };

  const {
    open,
    busy,
    tone,
    title,
    description,
    warning = '',
    confirmLabel,
    onOpenChange,
    onConfirm,
  }: Props = $props();

  function requestOpenChange(nextOpen: boolean): void {
    if (!busy || nextOpen) {
      onOpenChange(nextOpen);
    }
  }
</script>

<Dialog {open} onOpenChange={requestOpenChange}>
  <DialogContent closeLabel={t('common.close')} class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>{title}</DialogTitle>
      <DialogDescription>{description}</DialogDescription>
    </DialogHeader>

    {#if warning}
      <Alert variant={tone === 'destructive' ? 'destructive' : 'warning'} size="sm">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription class="whitespace-pre-line">{warning}</AlertDescription>
      </Alert>
    {/if}

    <DialogFooter>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={busy}
        onclick={() => {
          requestOpenChange(false);
        }}
      >
        {t('common.cancel')}
      </Button>
      <Button
        type="button"
        variant={tone === 'destructive' ? 'destructive' : 'default'}
        size="sm"
        disabled={busy}
        onclick={onConfirm}
      >
        {confirmLabel}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
