<script lang="ts">
  import { AddonStateMessage } from '@entities/addon';
  import { openExternal } from '@shared/api';
  import { t, translateKey } from '@shared/i18n';
  import { Button } from '@shared/ui';
  import ExternalLinkIcon from '@lucide/svelte/icons/external-link';

  import type { RenoDxStore } from '../model/create-renodx-store.svelte';
  import RenoDxExternalInstall from './RenoDxExternalInstall.svelte';

  type Props = {
    gameId: string;
    store: RenoDxStore;
    /** Global busy flag: any exclusive page-level operation is in flight. */
    busy: boolean;
  };

  const { gameId, store, busy }: Props = $props();

  const externalUrl = $derived(store.externalUrl);

  const externalLabel = $derived.by((): string => {
    const fallback = t('gameDetails.renodx.actionOpenExternal');

    return store.externalLabelKey ? translateKey(store.externalLabelKey, fallback) : fallback;
  });

  const externalLinkDisabled = $derived(busy || !externalUrl);

  function openExternalLink(): void {
    if (busy || !externalUrl) {
      return;
    }

    void openExternal(externalUrl);
  }
</script>

{#if store.externalFileInstallable}
  <RenoDxExternalInstall {gameId} {store} {busy} />
{:else}
  <AddonStateMessage icon="info" message={t('gameDetails.renodx.external')}>
    {#snippet actions()}
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={externalLinkDisabled}
        onclick={openExternalLink}
      >
        <ExternalLinkIcon class="size-4" aria-hidden="true" />
        {externalLabel}
      </Button>
    {/snippet}
  </AddonStateMessage>
{/if}
