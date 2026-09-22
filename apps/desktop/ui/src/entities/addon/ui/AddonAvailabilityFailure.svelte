<script lang="ts">
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import { Alert, AlertDescription, Button } from '@shared/ui';

  type Props = {
    /** Disables retry while another add-on operation is in progress. */
    disabled?: boolean;
    retrying?: boolean;
    onRetry: () => void;
  };

  const { disabled = false, retrying = false, onRetry }: Props = $props();
  const retryDisabled = $derived(disabled || retrying);
</script>

<Alert variant="warning" size="default" role="alert">
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
      <RefreshCwIcon class={cn('size-4', retrying && 'animate-spin')} aria-hidden="true" />
      {t('addon.availability.retry')}
    </Button>
    <span class="sr-only" role="status">
      {#if retrying}
        {t('addon.availability.checking')}
      {/if}
    </span>
  </AlertDescription>
</Alert>
