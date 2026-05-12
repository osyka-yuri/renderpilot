<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    children?: Snippet;
  };

  const { children, class: className = '', ...rest }: Props = $props();
</script>

<div
  {...rest}
  class={cn(
    'flex items-center justify-between gap-4 border-b border-border-subtle p-4',
    'last:border-b-0',
    'max-md:flex-col max-md:items-stretch max-md:gap-3',
    className,
  )}
>
  {@render children?.()}
</div>
