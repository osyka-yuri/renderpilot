<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  type Props = HTMLAttributes<HTMLDivElement> & {
    label?: string;
    children?: Snippet;
  };

  const { label = '', class: className = '', children, ...rest }: Props = $props();

  const labelText = $derived(label.trim());
</script>

<div
  {...rest}
  class={cn('min-w-0 rounded-2xl border border-border-subtle bg-bg-soft p-3', className)}
>
  {#if labelText}
    <dt class="mb-1 text-xs tracking-widest text-text-subtle uppercase">{labelText}</dt>
  {/if}

  <dd class="text-sm/tight font-semibold wrap-break-word text-text-strong">
    {@render children?.()}
  </dd>
</div>
