<script lang="ts">
  import {
    AddonStateMessage,
    AddonUninstallAction,
    createInstalledLabels,
    isMutationSuccess,
  } from '@entities/addon';
  import { t } from '@shared/i18n';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();
  const labels = createInstalledLabels('gameDetails.renodx');

  async function handleUninstall(): Promise<boolean> {
    if (busy || store.isInstalled || !store.hasPersistedRecord) {
      return false;
    }

    return isMutationSuccess(await store.uninstall(gameId));
  }
</script>

<div class="flex w-full flex-col gap-4">
  <AddonStateMessage
    tone="warning"
    icon="warning"
    message={t('gameDetails.renodx.inactiveInstallRecovery')}
  />
  <div class="flex w-full items-center">
    <AddonUninstallAction
      {busy}
      actionKey={labels.actionUninstall}
      confirmTitleKey={labels.uninstallConfirmTitle}
      confirmBodyKey={labels.uninstallConfirmBody}
      confirmActionKey={labels.uninstallConfirmAction}
      onConfirm={handleUninstall}
    />
  </div>
</div>
