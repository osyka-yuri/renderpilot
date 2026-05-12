<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    children?: Snippet;
  };

  const { class: className = '', children, ...rest }: Props = $props();
</script>

<div
  {...rest}
  class={cn(
    'rounded-2xl border border-dashed border-border-subtle bg-bg-card/60 p-4',
    'text-text-muted shadow-none',
    className,
  )}
>
  {@render children?.()}
</div>
