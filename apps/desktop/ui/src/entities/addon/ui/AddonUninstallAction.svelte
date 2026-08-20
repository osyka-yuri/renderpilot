<script lang="ts">
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import {
    AlertDialog,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
    Button,
  } from '@shared/ui';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';

  type ConfirmResult = boolean | undefined;

  type Props = {
    busy?: boolean;
    actionKey: MessageKeyWithoutParams;
    confirmTitleKey: MessageKeyWithoutParams;
    confirmBodyKey: MessageKeyWithoutParams;
    confirmActionKey: MessageKeyWithoutParams;
    onConfirm: () => ConfirmResult | Promise<ConfirmResult>;
  };

  let {
    busy = false,
    actionKey,
    confirmTitleKey,
    confirmBodyKey,
    confirmActionKey,
    onConfirm,
  }: Props = $props();

  let open = $state(false);
  let confirming = $state(false);

  const disabled = $derived(busy || confirming);

  function openConfirmDialog(): void {
    if (disabled) {
      return;
    }

    open = true;
  }

  async function confirmUninstall(): Promise<void> {
    if (disabled) {
      return;
    }

    confirming = true;

    try {
      const result = await onConfirm();
      if (result !== false) {
        open = false;
      }
    } finally {
      confirming = false;
    }
  }
</script>

<Button
  type="button"
  variant="destructive"
  size="sm"
  class="ms-auto"
  {disabled}
  aria-haspopup="dialog"
  aria-expanded={open}
  onclick={openConfirmDialog}
>
  <Trash2Icon class="size-4" aria-hidden="true" />
  {t(actionKey)}
</Button>

<AlertDialog bind:open>
  <AlertDialogContent class="sm:max-w-md" escapeKeydownBehavior={disabled ? 'ignore' : 'close'}>
    <AlertDialogHeader>
      <AlertDialogTitle>{t(confirmTitleKey)}</AlertDialogTitle>
      <AlertDialogDescription>
        {t(confirmBodyKey)}
      </AlertDialogDescription>
    </AlertDialogHeader>

    <AlertDialogFooter>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        {disabled}
        onclick={() => {
          open = false;
        }}
      >
        {t('common.cancel')}
      </Button>

      <Button type="button" variant="destructive" size="sm" {disabled} onclick={confirmUninstall}>
        <Trash2Icon class="size-4" aria-hidden="true" />
        {t(confirmActionKey)}
      </Button>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
