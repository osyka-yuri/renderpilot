<script lang="ts">
  import type { VoidHandler } from '@shared/callbacks';
  import type { HTMLAttributes } from 'svelte/elements';
  import { t } from '@shared/i18n';
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyTitle,
    Spinner,
  } from '@shared/ui';

  type Props = HTMLAttributes<HTMLDivElement> & {
    busy?: boolean;
    scanButtonLabel?: string;
    onScan?: VoidHandler;
  };

  const {
    busy = false,
    scanButtonLabel = '',
    onScan = () => undefined,
    class: className = '',
    ...rest
  }: Props = $props();

  const resolvedScanButtonLabel = $derived(scanButtonLabel.trim() || t('games.scanFolder'));
</script>

<Empty {...rest} class={className}>
  <EmptyHeader class="w-full">
    <EmptyTitle>{t('games.empty.title')}</EmptyTitle>
    <EmptyDescription>
      {t('games.empty.description')}
    </EmptyDescription>
  </EmptyHeader>

  <EmptyContent>
    <Button variant="default" size="sm" disabled={busy} onclick={onScan}>
      {#if busy}
        <Spinner />
      {/if}
      {resolvedScanButtonLabel}
    </Button>
  </EmptyContent>
</Empty>
