<script lang="ts">
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
  } from '@shared/ui';
  import SearchIcon from '@lucide/svelte/icons/search';

  type Props = {
    onResetFilters?: () => void;
  };

  const { onResetFilters = () => undefined }: Props = $props();
</script>

<div class="flex flex-1 flex-col items-center justify-center" aria-live="polite" aria-atomic="true">
  <Empty class="border-0">
    <EmptyHeader>
      <EmptyMedia variant="icon">
        <SearchIcon />
      </EmptyMedia>
      <EmptyTitle>No games match current filters</EmptyTitle>
      <EmptyDescription>
        Try adjusting your search or filter criteria, or reset filters to see all games.
      </EmptyDescription>
    </EmptyHeader>
    <EmptyContent>
      <Button variant="outline" size="sm" onclick={onResetFilters}>Reset Filters</Button>
    </EmptyContent>
  </Empty>
</div>
