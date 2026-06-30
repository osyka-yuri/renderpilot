<script lang="ts">
  import type { Snippet } from 'svelte';

  import { cn } from '@shared/classnames';

  type Props = {
    label: string;
    inline?: boolean;
    wrap?: boolean;
    gap?: 1.5 | 2;
    children: Snippet;
  };

  const { label, inline = true, wrap = true, gap = 2, children }: Props = $props();

  const containerClass = $derived(
    cn(
      inline ? 'inline-flex' : 'flex',
      wrap && 'flex-wrap',
      'items-center',
      gap === 1.5 ? 'gap-1.5' : 'gap-2',
    ),
  );
</script>

<div class={containerClass}>
  <span class="text-sm font-medium text-muted-foreground">{label}</span>
  {@render children()}
</div>
