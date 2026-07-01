<script lang="ts">
  import { t, translateKey } from '@shared/i18n';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxManualInstall from './RenoDxManualInstall.svelte';
  import RenoDxStateMessage from './RenoDxStateMessage.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag: any exclusive page-level operation is in flight. */
    busy: boolean;
    /** Which availability outcome this view is rendering. */
    variant: 'unsupported' | 'incompatible';
  };

  const { gameId, store, busy, variant }: Props = $props();

  const manualInstall = $derived(store.manualInstall);

  const incompatibleReason = $derived.by((): string => {
    if (store.outcome?.kind !== 'incompatible') {
      return '';
    }

    const reason = store.outcome.reason.reason;

    return translateKey(`gameDetails.renodx.reason.${reason}`, reason.replace(/_/g, ' '));
  });
</script>

<div class="flex w-full flex-col gap-3">
  {#if variant === 'unsupported'}
    <RenoDxStateMessage icon="unsupported" message={t('gameDetails.renodx.unsupported')} />
  {:else}
    <RenoDxStateMessage
      tone="warning"
      icon="warning"
      message={t('gameDetails.renodx.incompatible', { reason: incompatibleReason })}
    />
  {/if}

  {#if manualInstall}
    <RenoDxManualInstall {gameId} {store} manual={manualInstall} {busy} />
  {/if}
</div>
