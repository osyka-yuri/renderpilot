<script lang="ts">
  const FALLBACK_MONOGRAM = '—';

  type Props = {
    title?: string;
    coverBusy?: boolean;
    coverSrc?: string | null;
    monogram?: string;
  };

  let { title = '', coverBusy = false, coverSrc = null, monogram = '' }: Props = $props();

  const normalizedTitle = $derived(title.trim());
  const normalizedCoverSrc = $derived(coverSrc?.trim() ?? null);
  const coverLabel = $derived(
    normalizedTitle ? `Cover artwork: ${normalizedTitle}` : 'Cover artwork',
  );
  const placeholderLabel = $derived(monogram.trim() || FALLBACK_MONOGRAM);
</script>

<div class="cover-stack" role="img" aria-busy={coverBusy} aria-label={coverLabel}>
  {#if normalizedCoverSrc}
    <img
      class="cover-image"
      src={normalizedCoverSrc}
      alt=""
      loading="lazy"
      decoding="async"
      draggable="false"
    />
  {:else}
    <div class="cover-placeholder" aria-hidden="true">
      <span>{placeholderLabel}</span>
    </div>
  {/if}

  {#if coverBusy}
    <div class="cover-busy-overlay" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .cover-stack {
    position: relative;
    width: 100%;
    aspect-ratio: 600 / 900;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--accent-outline) 48%, var(--border-subtle));
    border-radius: var(--radius-lg);
    background: var(--bg-control);
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 10%, transparent);
  }

  .cover-image,
  .cover-placeholder {
    width: 100%;
    height: 100%;
    min-height: 0;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  .cover-image {
    display: block;
    object-fit: cover;
    object-position: center top;
    background: var(--bg-control);
    pointer-events: none;
    user-select: none;
  }

  .cover-placeholder {
    display: grid;
    place-items: center;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--accent) 16%, var(--bg-control)) 0%,
      var(--bg-control) 100%
    );
    color: var(--text-strong);
    pointer-events: none;
    user-select: none;
  }

  .cover-placeholder span {
    font-size: 1.45rem;
    font-weight: 600;
    line-height: 1;
    letter-spacing: 0.04em;
  }

  .cover-busy-overlay {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: color-mix(in srgb, var(--bg-card) 45%, transparent);
    pointer-events: none;
  }

  @media (max-width: 720px) {
    .cover-stack {
      width: min(7.125rem, 40vw);
      justify-self: start;
    }

    .cover-placeholder span {
      font-size: 1.2rem;
    }
  }
</style>
