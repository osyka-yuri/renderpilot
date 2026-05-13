<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@shared/ui';

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

<article {...rest} class={className} aria-labelledby={headingId} aria-label={articleLabel}>
  <Card>
    {#if hasHeader}
      <CardHeader>
        {#if eyebrowText}
          <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
            {eyebrowText}
          </p>
        {/if}

        {#if titleText}
          <CardTitle id={headingId}>{titleText}</CardTitle>
        {/if}

        {#if descriptionText}
          <CardDescription>{descriptionText}</CardDescription>
        {/if}
      </CardHeader>
    {/if}

    <CardContent>
      {@render children?.()}
    </CardContent>
  </Card>
</article>
