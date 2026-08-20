<script lang="ts">
  import type { D3d12ExecutableMutationAction } from '@shared/model';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
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
  import { t } from '@shared/i18n';

  type Props = {
    open: boolean;
    busy: boolean;
    actions: D3d12ExecutableMutationAction[];
    reason?: 'swap' | 'update_all';
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
  };

  const { open, busy, actions, reason = 'swap', onOpenChange, onConfirm }: Props = $props();
  const includesIntegrityChange = $derived(
    actions.some(
      (action) =>
        action.kind === 'patch' && action.current_sdk_version === action.original_sdk_version,
    ),
  );

  function requestOpenChange(next: boolean): void {
    if (!busy || next) {
      onOpenChange(next);
    }
  }
</script>

<Dialog {open} onOpenChange={requestOpenChange}>
  <DialogContent closeLabel={t('common.close')}>
    <DialogHeader>
      <DialogTitle>{t('gameDetails.d3d12.confirm.title')}</DialogTitle>
      <DialogDescription>
        {reason === 'update_all'
          ? t('gameDetails.d3d12.confirm.updateAllDescription')
          : t('gameDetails.d3d12.confirm.description')}
      </DialogDescription>
    </DialogHeader>

    <div class="grid max-h-72 gap-3 overflow-y-auto">
      {#each actions as action (action.executable_path)}
        <div class="grid gap-1 rounded-md border bg-muted/30 p-3 text-sm">
          <p class="font-medium break-all">{action.executable_path}</p>
          <p>
            {action.kind === 'restore'
              ? t('gameDetails.d3d12.action.planRestore', {
                  from: action.current_sdk_version,
                  to: action.target_sdk_version,
                })
              : t('gameDetails.d3d12.action.planPatch', {
                  from: action.current_sdk_version,
                  to: action.target_sdk_version,
                })}
          </p>
          <p class="text-xs break-all text-muted-foreground">
            {action.backup_exists
              ? t('gameDetails.d3d12.confirm.backupExists', { path: action.backup_path })
              : t('gameDetails.d3d12.confirm.backupWillCreate', { path: action.backup_path })}
          </p>
        </div>
      {/each}
    </div>

    {#if includesIntegrityChange}
      <Alert variant="warning" size="sm" role="note">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{t('gameDetails.d3d12.confirm.signatureWarning')}</AlertDescription>
      </Alert>
    {/if}

    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        disabled={busy}
        onclick={() => {
          requestOpenChange(false);
        }}
      >
        {t('common.cancel')}
      </Button>
      <Button size="sm" disabled={busy} onclick={onConfirm}>
        {t('gameDetails.d3d12.confirm.accept')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
