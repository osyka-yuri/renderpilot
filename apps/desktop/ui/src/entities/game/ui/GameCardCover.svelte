<script lang="ts">
  import { AspectRatio, Spinner } from '@shared/ui';

  const FALLBACK_MONOGRAM = '—';

  type Props = {
    title?: string;
    coverBusy?: boolean;
    coverSrc?: string | null;
    monogram?: string;
  };

  let {
    title = '',
    coverBusy = false,
    coverSrc = null,
    monogram = '',
  }: Props = $props();

  let failedCoverSrc = $state<string | null>(null);

  const normalizedTitle = $derived(normalizeOptionalText(title) ?? '');
  const normalizedCoverSrc = $derived(normalizeOptionalText(coverSrc));
  const placeholderLabel = $derived(normalizeOptionalText(monogram) ?? FALLBACK_MONOGRAM);

  const coverAlt = $derived(
    normalizedTitle ? `Cover artwork: ${normalizedTitle}` : 'Cover artwork',
  );

  function normalizeOptionalText(value: string | null | undefined): string | null {
    const trimmedValue = value?.trim();

    return trimmedValue ?? null;
  }

  function handleCoverLoadError(event: Event): void {
    const imageElement = event.currentTarget;

    if (!(imageElement instanceof HTMLImageElement)) {
      failedCoverSrc = normalizedCoverSrc;
      return;
    }

    failedCoverSrc = normalizeOptionalText(imageElement.currentSrc || imageElement.src);
  }
</script>

<AspectRatio
  class="relative overflow-hidden rounded-xl bg-muted"
  ratio={2 / 3}
>
  {#if normalizedCoverSrc !== null && normalizedCoverSrc !== failedCoverSrc}
    <img
      class="block size-full object-cover object-top"
      src={normalizedCoverSrc}
      alt={coverAlt}
      loading="lazy"
      decoding="async"
      draggable="false"
      onerror={handleCoverLoadError}
    />
  {:else}
    <div class="grid size-full place-items-center text-foreground" aria-hidden="true">
      <span class="max-md:text-lg text-2xl font-semibold leading-none tracking-wider">
        {placeholderLabel}
      </span>
    </div>
  {/if}

  {#if coverBusy}
    <div
      class="pointer-events-none absolute inset-0 grid place-items-center bg-background/60"
      aria-hidden="true"
    >
      <Spinner />
    </div>
  {/if}
</AspectRatio>