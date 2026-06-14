<script lang="ts">
  import { Progress } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { cn } from '@shared/classnames';

  type Props = {
    /** Aggregate progress so far (e.g. summed download fractions across a batch). */
    value: number;
    /** Total the bar fills towards (e.g. the number of artifacts in the batch). */
    max: number;
    /** Whether the owning control is in a busy/downloading state. */
    active: boolean;
    /** Accessible label; defaults to the generic "Download progress". */
    ariaLabel?: string;
    class?: string;
  };

  const { value, max, active, ariaLabel, class: className }: Props = $props();
</script>

{#if active && max > 0}
  <div class={cn('w-16', className)}>
    <Progress {value} {max} aria-label={ariaLabel ?? t('common.downloadProgress')} />
  </div>
{/if}
