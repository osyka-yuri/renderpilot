<script lang="ts">
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import { cn } from '@shared/classnames';
  import type { SettingsMessageKind } from '@entities/settings';

  type Props = {
    message?: string;
    kind?: SettingsMessageKind | null;
    id?: string;
  };

  const { message = '', kind = null, id }: Props = $props();
</script>

{#if message}
  <p
    {id}
    class={cn(
      'flex items-center gap-1.5 text-xs',
      kind === 'success' && 'text-emerald-600 dark:text-emerald-400',
      kind === 'error' && 'text-destructive',
      kind === null && 'text-muted-foreground',
    )}
    role="status"
    aria-live="polite"
  >
    {#if kind === 'success'}
      <CircleCheckIcon class="size-3.5 shrink-0" aria-hidden="true" />
    {:else if kind === 'error'}
      <CircleXIcon class="size-3.5 shrink-0" aria-hidden="true" />
    {/if}
    {message}
  </p>
{/if}
