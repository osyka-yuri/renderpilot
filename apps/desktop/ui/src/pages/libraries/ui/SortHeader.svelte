<script lang="ts">
  import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
  import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import { Button, type SortableTableFeatures } from '@shared/ui';
  import { useSelector } from '@tanstack/svelte-store';
  import type { Column } from '@tanstack/table-core';
  import { untrack } from 'svelte';
  import type { LibraryPackageRow } from '../model/libraries-page-model';

  type Props = {
    label: string;
    column: Column<SortableTableFeatures, LibraryPackageRow>;
    class?: string;
  };

  let { label, column, class: className = '' }: Props = $props();

  const tableState = useSelector(untrack(() => column.table.store));
  const sortState = $derived.by(() => {
    void tableState.current;
    return column.getIsSorted();
  });
  const canSort = $derived(column.getCanSort());

  const sortButtonLabel = $derived(t('libraries.sort.byColumn', { label }));

  function handleSortClick(): void {
    if (!canSort) {
      return;
    }

    column.toggleSorting();
  }
</script>

<Button
  type="button"
  variant="ghost"
  size="sm"
  class={cn('select-none', className)}
  disabled={!canSort}
  aria-label={sortButtonLabel}
  onclick={handleSortClick}
>
  <span>{label}</span>

  {#if sortState === 'asc'}
    <ArrowUpIcon class="size-3 shrink-0" aria-hidden="true" />
  {:else if sortState === 'desc'}
    <ArrowDownIcon class="size-3 shrink-0" aria-hidden="true" />
  {/if}
</Button>
