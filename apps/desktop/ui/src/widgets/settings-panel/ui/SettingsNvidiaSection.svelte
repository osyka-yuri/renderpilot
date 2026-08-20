<script lang="ts">
  import { onMount } from 'svelte';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import {
    Card,
    CardContent,
    CardHeader,
    Empty,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
    Skeleton,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import {
    type DlssIndicatorContext,
    type GlobalNvidiaPresetsContext,
  } from '@features/nvapi-settings';
  import DlssIndicatorCard from './DlssIndicatorCard.svelte';
  import GlobalNvidiaPresetsCard from './GlobalNvidiaPresetsCard.svelte';

  type Props = {
    dlssIndicator: DlssIndicatorContext;
    globalPresets: GlobalNvidiaPresetsContext;
  };

  const { dlssIndicator, globalPresets }: Props = $props();

  // Both sub-features load once on mount; show a skeleton until both settle so
  // the optimistic `supported`/`nvapiAvailable` defaults don't flash a card.
  const isLoading = $derived(!dlssIndicator.loaded || !globalPresets.loaded);
  const isUnsupported = $derived(!dlssIndicator.supported && !globalPresets.nvapiAvailable);

  onMount(() => {
    if (!dlssIndicator.loaded) {
      void dlssIndicator.load();
    }
    if (!globalPresets.loaded) {
      void globalPresets.load();
    }
  });
</script>

{#if isLoading}
  <Card>
    <CardHeader class="gap-2">
      <Skeleton class="h-5 w-44" />
      <Skeleton class="h-4 w-64" />
    </CardHeader>
    <CardContent class="grid gap-2">
      <Skeleton class="h-14 w-full rounded-md" />
      <Skeleton class="h-14 w-full rounded-md" />
    </CardContent>
  </Card>
{:else if isUnsupported}
  <Empty class="border">
    <EmptyHeader>
      <EmptyMedia variant="icon">
        <CpuIcon aria-hidden="true" />
      </EmptyMedia>
      <EmptyTitle level={2}>{t('settings.nvidia.unsupported.title')}</EmptyTitle>
      <EmptyDescription>{t('settings.nvidia.unsupported.description')}</EmptyDescription>
    </EmptyHeader>
  </Empty>
{:else}
  {#if dlssIndicator.supported}
    <DlssIndicatorCard {dlssIndicator} />
  {/if}

  {#if globalPresets.nvapiAvailable}
    <GlobalNvidiaPresetsCard {globalPresets} />
  {/if}
{/if}
