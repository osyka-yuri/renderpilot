<script lang="ts">
  import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
  import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
  import { cn } from '@shared/classnames';
  import type { Column } from '@tanstack/table-core';
  import type { LibraryManifestEntry } from '../model/libraries-page-model';

  type SortState = false | 'asc' | 'desc';

  type Props = {
    label: string;
    column: Column<LibraryManifestEntry>;
    class?: string;
  };

  let { label, column, class: className = '' }: Props = $props();

  const sortState = $derived(column.getIsSorted());
  const canSort = $derived(column.getCanSort());

  const sortButtonLabel = $derived(getSortButtonLabel(label, sortState));

  function getSortButtonLabel(label: string, state: SortState): string {
    switch (state) {
      case 'asc':
        return `${label}: sorted ascending. Click to change sort.`;

      case 'desc':
        return `${label}: sorted descending. Click to change sort.`;

      default:
        return `${label}: not sorted. Click to sort.`;
    }
  }

  function handleSortClick(): void {
    if (!canSort) return;

    column.toggleSorting();
  }
</script>

<button
  type="button"
  class={cn(
    'flex items-center gap-1 select-none',
    canSort ? 'cursor-pointer' : 'cursor-default opacity-60',
    className,
  )}
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
</button>
