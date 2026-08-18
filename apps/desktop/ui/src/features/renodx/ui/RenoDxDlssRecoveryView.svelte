<script lang="ts">
  import { AddonStateMessage } from '@entities/addon';
  import { t } from '@shared/i18n';
  import { Button } from '@shared/ui';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  function retryRecovery(): void {
    if (busy) {
      return;
    }

    void store.retryDlssFixRecovery(gameId);
  }
</script>

<AddonStateMessage
  tone="warning"
  icon="warning"
  message={t('gameDetails.renodx.dlssFixRecoveryPending')}
>
  {#snippet actions()}
    <Button type="button" variant="outline" size="sm" disabled={busy} onclick={retryRecovery}>
      {t('gameDetails.renodx.actionFinishDlssFixRecovery')}
    </Button>
  {/snippet}
</AddonStateMessage>
