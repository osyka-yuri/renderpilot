<script lang="ts">
  import { Button, Input } from '@shared/ui';
  import { cn } from '@shared/utils';

  type SearchChangeHandler = (value: string) => void;
  type ToggleFiltersHandler = () => void;

  type Props = {
    searchQuery?: string;
    hasFilterIndicator?: boolean;
    onSearchChange?: SearchChangeHandler;
    onToggleFilters?: ToggleFiltersHandler;
  };

  const SEARCH_LABEL = 'Search games';
  const SEARCH_PLACEHOLDER = 'Search games';

  const FILTERS_BUTTON_LABEL = 'Open library filters';
  const FILTERS_BUTTON_ACTIVE_LABEL = 'Open library filters, filters active';

  const {
    searchQuery = '',
    hasFilterIndicator = false,
    onSearchChange,
    onToggleFilters,
  }: Props = $props();

  const filtersButtonLabel = $derived(
    hasFilterIndicator ? FILTERS_BUTTON_ACTIVE_LABEL : FILTERS_BUTTON_LABEL,
  );

  function handleSearchChange(nextSearchQuery: string): void {
    onSearchChange?.(nextSearchQuery);
  }

  function handleFiltersToggle(): void {
    onToggleFilters?.();
  }
</script>

<div class={cn('flex items-center justify-end gap-2', 'max-md:justify-stretch')} role="search">
  <label
    class={cn('block max-w-88 min-w-48 shrink grow basis-88', 'max-md:max-w-none max-md:min-w-0')}
  >
    <span class="sr-only">
      {SEARCH_LABEL}
    </span>

    <Input
      type="search"
      placeholder={SEARCH_PLACEHOLDER}
      value={searchQuery}
      onValueChange={handleSearchChange}
    />
  </label>

  <div class="relative inline-flex flex-none">
    <Button
      aria-label={filtersButtonLabel}
      aria-haspopup="dialog"
      iconOnly
      variant="secondary"
      size="sm"
      onclick={handleFiltersToggle}
    >
      <svg class="size-4.5" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path
          d="M4 6h16l-6.5 7.2v4.9l-3 1.9v-6.8z"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </Button>

    {#if hasFilterIndicator}
      <span
        class={cn(
          'pointer-events-none absolute -top-0.5 -right-0.5 size-2 rounded-full',
          'bg-accent ring-2 ring-bg-card',
        )}
        aria-hidden="true"
      ></span>
    {/if}
  </div>
</div>
