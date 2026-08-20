<script lang="ts" generics="TComponent">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { onMount, type Snippet } from 'svelte';

  import { t } from '@shared/i18n';
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
    Spinner,
  } from '@shared/ui';

  import type { LazyPageResource } from './lazy-page-resource.svelte';

  type Props = {
    page: LazyPageResource<TComponent>;
    onBack: () => void;
    children: Snippet<[TComponent]>;
  };

  const { page, onBack, children }: Props = $props();
  const state = $derived(page.state);

  onMount(() => {
    void page.activate();
  });
</script>

{#if state.status === 'ready'}
  {@render children(state.component)}
{:else if state.status === 'error'}
  <div class="flex flex-1 flex-col items-center justify-center p-6" role="alert">
    <Empty class="border-0">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TriangleAlertIcon aria-hidden="true" />
        </EmptyMedia>
        <EmptyTitle level={1}>{t('pageLoad.error.title')}</EmptyTitle>
        <EmptyDescription>{t('pageLoad.error.description')}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent class="flex-row justify-center">
        <Button
          variant="default"
          size="sm"
          onclick={() => {
            void page.retry();
          }}
        >
          {t('pageLoad.error.retry')}
        </Button>
        <Button variant="outline" size="sm" onclick={onBack}>
          {t('pageLoad.error.backToGames')}
        </Button>
      </EmptyContent>
    </Empty>
  </div>
{:else}
  <div
    class="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground"
    role="status"
    aria-live="polite"
  >
    <Spinner />
    <span>{t('pageLoad.loading')}</span>
  </div>
{/if}
