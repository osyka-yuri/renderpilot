<script lang="ts">
  import { cva } from 'class-variance-authority';
  import { cn } from '@shared/utils';
  import type { BadgeSurface, BadgeSize, BadgeTone } from './badge-types';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  const badgeVariants = cva(
    'inline-flex w-fit max-w-full items-center justify-center gap-1.5 border align-middle leading-none font-semibold whitespace-nowrap',
    {
      variants: {
        tone: {
          neutral: 'text-text-strong',
          muted: 'text-text-muted',
          success: 'text-success',
          warning: 'text-warning',
          danger: 'text-danger',
        },
        surface: {
          soft: 'border-border-subtle bg-bg-control',
          outline: 'border-border-subtle bg-bg-card/70',
        },
        size: {
          sm: 'min-h-5 rounded-2xl px-2 py-1 text-xs',
          md: 'min-h-7 rounded-2xl px-2.5 py-1.5 text-xs',
        },
        pill: {
          true: 'rounded-full',
          false: '',
        },
        multiline: {
          true: 'text-center leading-tight whitespace-normal',
          false: '',
        },
      },
      compoundVariants: [
        {
          surface: 'soft',
          tone: 'success',
          class: 'border-success/25 bg-success/10',
        },
        {
          surface: 'soft',
          tone: 'warning',
          class: 'border-warning/25 bg-warning/10',
        },
        {
          surface: 'soft',
          tone: 'danger',
          class: 'border-danger/25 bg-danger/10',
        },
      ],
      defaultVariants: {
        tone: 'neutral',
        surface: 'soft',
        size: 'sm',
        pill: false,
        multiline: false,
      },
    },
  );

  type Props = HTMLAttributes<HTMLSpanElement> & {
    tone?: BadgeTone;
    surface?: BadgeSurface;
    size?: BadgeSize;
    pill?: boolean;
    dot?: boolean;
    multiline?: boolean;
    children?: Snippet;
  };

  const {
    tone = 'neutral',
    surface = 'soft',
    size = 'sm',
    pill = false,
    dot = false,
    multiline = false,
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const classes = $derived(cn(badgeVariants({ tone, surface, size, pill, multiline }), className));
</script>

<span {...rest} class={classes}>
  {#if dot}
    <span class="size-1 shrink-0 rounded-full bg-current opacity-70" aria-hidden="true"></span>
  {/if}

  {@render children?.()}
</span>
