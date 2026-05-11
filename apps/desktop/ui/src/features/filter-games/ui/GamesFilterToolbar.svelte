<script lang="ts">
  import { Button, Input } from '@shared/ui';

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

  let {
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

<div class="filters-search-row" role="search">
  <label class="search-field">
    <span class="visually-hidden">{SEARCH_LABEL}</span>

    <Input
      type="search"
      placeholder={SEARCH_PLACEHOLDER}
      value={searchQuery}
      onValueChange={handleSearchChange}
    />
  </label>

  <div class="filters-trigger-shell">
    <Button
      aria-label={filtersButtonLabel}
      aria-haspopup="dialog"
      iconOnly
      variant="secondary"
      size="sm"
      onclick={handleFiltersToggle}
    >
      <svg class="filters-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
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
      <span class="filters-active-dot" aria-hidden="true"></span>
    {/if}
  </div>
</div>

<style>
  .filters-search-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .search-field {
    display: block;
    flex: 1 1 22rem;
    min-width: 12rem;
    max-width: 22rem;
  }

  .filters-trigger-shell {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
  }

  .filters-icon {
    width: 1.125rem;
    height: 1.125rem;
  }

  .filters-active-dot {
    position: absolute;
    top: -0.15rem;
    right: -0.15rem;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 999px;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--bg-card);
    pointer-events: none;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    white-space: nowrap;
    clip: rect(0, 0, 0, 0);
    clip-path: inset(50%);
    border: 0;
  }

  @media (max-width: 760px) {
    .filters-search-row {
      justify-content: stretch;
    }

    .search-field {
      min-width: 0;
      max-width: none;
    }
  }
</style>
