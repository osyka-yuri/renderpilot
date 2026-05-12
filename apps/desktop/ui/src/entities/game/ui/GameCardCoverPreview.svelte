<script lang="ts">
  import { cn } from '@shared/utils';

  const FALLBACK_MONOGRAM = '—';

  type Props = {
    title?: string;
    coverBusy?: boolean;
    coverSrc?: string | null;
    monogram?: string;
  };

  const { title = '', coverBusy = false, coverSrc = null, monogram = '' }: Props = $props();

  const normalizedTitle = $derived(title.trim());
  const normalizedCoverSrc = $derived(coverSrc?.trim() ?? null);
  const coverLabel = $derived(
    normalizedTitle ? `Cover artwork: ${normalizedTitle}` : 'Cover artwork',
  );
  const placeholderLabel = $derived(monogram.trim() || FALLBACK_MONOGRAM);
</script>

<div
  class={cn(
    'relative aspect-600/900 w-full overflow-hidden rounded-2xl border',
    'border-accent/30 bg-bg-control',
    'max-md:w-28 max-md:justify-self-start',
  )}
  role="img"
  aria-busy={coverBusy}
  aria-label={coverLabel}
>
  {#if normalizedCoverSrc}
    <img
      class={cn(
        'pointer-events-none block size-full min-h-0 rounded-none bg-bg-control',
        'object-cover object-top select-none',
      )}
      src={normalizedCoverSrc}
      alt=""
      loading="lazy"
      decoding="async"
      draggable="false"
    />
  {:else}
    <div
      class={cn(
        'pointer-events-none grid size-full min-h-0 place-items-center',
        'rounded-none bg-bg-control text-text-strong select-none',
      )}
      aria-hidden="true"
    >
      <span class={cn('text-2xl leading-none font-semibold tracking-wider', 'max-md:text-lg')}>
        {placeholderLabel}
      </span>
    </div>
  {/if}

  {#if coverBusy}
    <div
      class={cn('pointer-events-none absolute inset-0 rounded-[inherit] bg-bg-card/45')}
      aria-hidden="true"
    ></div>
  {/if}
</div>
