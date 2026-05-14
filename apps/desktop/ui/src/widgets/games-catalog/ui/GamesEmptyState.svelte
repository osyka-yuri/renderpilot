<script lang="ts">
  import { cn } from '@shared/classnames';
  import type { VoidHandler } from '@shared/callbacks';
  import type { HTMLAttributes } from 'svelte/elements';
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
    onRefresh?: VoidHandler;
    onScan?: VoidHandler;
  };

  const {
    busy = false,
    scanButtonLabel = 'Scan Folder',
    onRefresh = () => undefined,
    onScan = () => undefined,
    class: className = '',
    ...rest
  }: Props = $props();
</script>

<Empty {...rest} class={cn(className)}>
  <EmptyHeader>
    <EmptyTitle>No scanned games yet</EmptyTitle>
    <EmptyDescription>
      Select a game folder to populate the dashboard with components, updates, backup state, and
      quick actions.
    </EmptyDescription>
  </EmptyHeader>

  <EmptyContent
    class={cn(
      'flex-row flex-wrap items-start gap-2',
      'max-sm:w-full max-sm:flex-col-reverse max-sm:items-stretch',
    )}
  >
    <Button variant="secondary" size="sm" disabled={busy} onclick={onRefresh}>
      {#if busy}
        <Spinner />
      {/if}
      Refresh Libraries
    </Button>

    <Button variant="default" size="sm" disabled={busy} onclick={onScan}>
      {#if busy}
        <Spinner />
      {/if}
      {scanButtonLabel}
    </Button>
  </EmptyContent>
</Empty>
