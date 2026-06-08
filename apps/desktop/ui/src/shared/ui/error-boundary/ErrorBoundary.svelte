<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
  } from '@shared/ui';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { t } from '@shared/i18n';

  type Props = {
    /** Content protected by the boundary. */
    children: Snippet;
  };

  const { children }: Props = $props();

  function reportError(error: unknown): void {
    // Surface the crash for diagnostics; the `failed` snippet renders the
    // recoverable fallback UI so a render error never white-screens the app.
    console.error('Unhandled UI error caught by ErrorBoundary:', error);
  }
</script>

<svelte:boundary onerror={reportError}>
  {@render children()}

  {#snippet failed(_error, reset)}
    <div class="flex flex-1 flex-col items-center justify-center p-6" role="alert">
      <Empty class="border-0">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <TriangleAlertIcon aria-hidden="true" />
          </EmptyMedia>
          <EmptyTitle>{t('error.boundary.title')}</EmptyTitle>
          <EmptyDescription>{t('error.boundary.description')}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button variant="default" size="sm" onclick={reset}>
            {t('error.boundary.reset')}
          </Button>
        </EmptyContent>
      </Empty>
    </div>
  {/snippet}
</svelte:boundary>
