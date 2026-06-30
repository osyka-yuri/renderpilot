<script lang="ts">
  import { t } from '@shared/i18n';
  import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
  } from '@shared/ui';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  type Props = {
    busy: boolean;
    onConfirm: () => void;
  };

  const { busy, onConfirm }: Props = $props();

  let open = $state(false);

  function requestUninstall(): void {
    open = true;
  }

  function setOpen(nextOpen: boolean): void {
    open = nextOpen;
  }

  function cancelUninstall(): void {
    open = false;
  }

  function confirmUninstall(): void {
    open = false;
    onConfirm();
  }
</script>

<Button variant="destructive" size="sm" class="ml-auto" disabled={busy} onclick={requestUninstall}>
  <Trash2Icon class="size-4" aria-hidden="true" />
  {t('gameDetails.renodx.actionUninstall')}
</Button>

<Dialog {open} onOpenChange={setOpen}>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>{t('gameDetails.renodx.uninstallConfirmTitle')}</DialogTitle>
      <DialogDescription>
        {t('gameDetails.renodx.uninstallConfirmBody')}
      </DialogDescription>
    </DialogHeader>
    <DialogFooter>
      <Button variant="secondary" size="sm" onclick={cancelUninstall}>
        {t('common.cancel')}
      </Button>
      <Button variant="destructive" size="sm" disabled={busy} onclick={confirmUninstall}>
        <Trash2Icon class="size-4" aria-hidden="true" />
        {t('gameDetails.renodx.uninstallConfirmAction')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
