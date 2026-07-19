<script lang="ts">
  import { t } from '@shared/i18n';
  import { Alert, AlertDescription, Button, Spinner } from '@shared/ui';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  type Props = {
    /** Disables retry while another add-on operation is in progress. */
    disabled?: boolean;
    retrying?: boolean;
    onRetry: () => void;
  };

  const { disabled = false, retrying = false, onRetry }: Props = $props();
  const retryDisabled = $derived(disabled || retrying);
</script>

<Alert variant="warning" size="default">
  <TriangleAlertIcon aria-hidden="true" />
  <AlertDescription>
    <p>{t('addon.availability.loadFailed')}</p>
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={retryDisabled}
      aria-busy={retrying}
      onclick={onRetry}
    >
      {#if retrying}
        <Spinner class="size-4" />
        {t('addon.availability.checking')}
      {:else}
        {t('addon.availability.retry')}
      {/if}
    </Button>
  </AlertDescription>
</Alert>
