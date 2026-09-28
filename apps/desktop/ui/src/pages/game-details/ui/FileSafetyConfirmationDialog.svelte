<script lang="ts">
  import type { D3d12ExecutableMutationAction } from '@shared/model';
  import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    ScrollArea,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { FileSafetyNotice } from '../model/file-safety-notice-policy';
  import FileSafetyAssessmentDetails from './FileSafetyAssessmentDetails.svelte';
  import D3d12ExecutableActionDetails from './D3d12ExecutableActionDetails.svelte';

  type Props = {
    notice: FileSafetyNotice | null;
    actions: readonly D3d12ExecutableMutationAction[];
    isUpdateAll?: boolean;
    onCancel: () => void;
    onConfirm: (rememberGeneralWarning: boolean) => void;
  };

  let { notice, actions, isUpdateAll = false, onCancel, onConfirm }: Props = $props();
  let rememberGeneralWarning = $state(false);
  let previousNotice: FileSafetyNotice | null = null;

  $effect(() => {
    if (notice !== previousNotice) {
      previousNotice = notice;
      rememberGeneralWarning = false;
    }
  });

  const title = $derived(
    actions.length > 0
      ? t('gameDetails.d3d12.confirm.title')
      : t('gameDetails.fileSafety.confirmTitle'),
  );
  const confirmLabel = $derived.by(() => {
    if (isUpdateAll) {
      return t('gameDetails.updateAll.action');
    }
    if (actions.length === 0) {
      return t('gameDetails.fileSafety.confirmChangeAction');
    }
    const patches = actions.some((action) => action.kind === 'patch');
    const restores = actions.some((action) => action.kind === 'restore');
    if (patches && restores) {
      return t('gameDetails.fileSafety.confirmExeMixedAction');
    }
    return t(
      patches
        ? 'gameDetails.fileSafety.confirmExePatchAction'
        : 'gameDetails.fileSafety.confirmExeRestoreAction',
    );
  });
</script>

<Dialog
  open={notice !== null || actions.length > 0}
  onOpenChange={(open) => {
    if (!open) {
      onCancel();
    }
  }}
>
  <DialogContent
    closeLabel={t('common.close')}
    class="max-h-[calc(100dvh-2rem)] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden sm:max-w-xl"
  >
    <DialogHeader>
      <DialogTitle>{title}</DialogTitle>
      <DialogDescription class="text-foreground">
        {t('gameDetails.fileSafety.generic')}
      </DialogDescription>
    </DialogHeader>

    <ScrollArea class="min-h-0" viewportRegion={{ label: title }} viewportFocusable>
      <div class="space-y-3">
        {#if notice?.kind === 'detected'}
          <FileSafetyAssessmentDetails assessment={notice.assessment} />
        {/if}

        {#if actions.length > 0}
          <D3d12ExecutableActionDetails {actions} />
        {/if}

        {#if notice?.kind === 'general'}
          <label
            class="flex cursor-pointer items-start gap-2 text-sm"
            class:border-t={actions.length > 0}
            class:pt-3={actions.length > 0}
          >
            <Checkbox bind:checked={rememberGeneralWarning} class="mt-0.5" />
            <span>{t('gameDetails.fileSafety.skipGeneralRiskConfirmation')}</span>
          </label>
        {/if}
      </div>
    </ScrollArea>

    <DialogFooter>
      <Button type="button" variant="secondary" size="sm" onclick={onCancel}>
        {t('common.cancel')}
      </Button>
      <Button
        type="button"
        size="sm"
        onclick={() => {
          onConfirm(rememberGeneralWarning);
        }}
      >
        {confirmLabel}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
