<script lang="ts">
  import { cn } from '@shared/utils';
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { SectionHeader, Surface } from '@shared/ui';

  type Props = HTMLAttributes<HTMLElement> & {
    titleId?: string;
    eyebrow?: string;
    title?: string;
    description?: string;
    children?: Snippet;
  };

  const {
    titleId = '',
    eyebrow = '',
    title = '',
    description = '',
    children,
    class: className = '',
    ...rest
  }: Props = $props();

  const toOptionalText = (value: string): string | undefined => {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  };

  const eyebrowText = $derived(toOptionalText(eyebrow));
  const titleText = $derived(toOptionalText(title));
  const descriptionText = $derived(toOptionalText(description));
  const normalizedTitleId = $derived(toOptionalText(titleId));

  const headingId = $derived(titleText ? normalizedTitleId : undefined);
  const articleLabel = $derived(titleText && !headingId ? titleText : undefined);
  const hasHeader = $derived(Boolean(eyebrowText ?? titleText ?? descriptionText));
</script>

<article
  {...rest}
  class={cn('grid gap-3', className)}
  aria-labelledby={headingId}
  aria-label={articleLabel}
>
  {#if hasHeader}
    <SectionHeader
      eyebrow={eyebrowText}
      title={titleText}
      titleId={headingId}
      description={descriptionText}
      class="max-md:px-0"
    />
  {/if}

  <div class="grid gap-0 overflow-hidden rounded-2xl">
    <Surface tone="elevated" shadow>
      {@render children?.()}
    </Surface>
  </div>
</article>
