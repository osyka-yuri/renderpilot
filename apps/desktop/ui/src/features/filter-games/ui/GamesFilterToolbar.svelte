<script lang="ts">
  import FunnelIcon from '@lucide/svelte/icons/funnel';
  import { Button, Input } from '@shared/ui';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';

  type SearchChangeHandler = (value: string) => void;
  type ToggleFiltersHandler = () => void;

  type Props = {
    searchQuery?: string;
    hasFilterIndicator?: boolean;
    onSearchChange?: SearchChangeHandler;
    onToggleFilters?: ToggleFiltersHandler;
  };

  const {
    searchQuery = '',
    hasFilterIndicator = false,
    onSearchChange = () => undefined,
    onToggleFilters = () => undefined,
  }: Props = $props();

  const filtersButtonLabel = $derived(
    hasFilterIndicator ? t('games.openFiltersActive') : t('games.openFilters'),
  );

  function handleSearchInput(
    event: Event & { currentTarget: EventTarget & HTMLInputElement },
  ): void {
    onSearchChange(event.currentTarget.value);
  }
</script>

<div class={cn('flex items-center justify-end gap-2', 'max-md:justify-stretch')} role="search">
  <label
    class={cn('block max-w-88 min-w-48 shrink grow basis-88', 'max-md:max-w-none max-md:min-w-0')}
  >
    <span class="sr-only">
      {t('games.search')}
    </span>

    <Input
      type="search"
      placeholder={t('games.search')}
      value={searchQuery}
      oninput={handleSearchInput}
    />
  </label>

  <div class="relative inline-flex flex-none">
    <Button
      aria-label={filtersButtonLabel}
      aria-haspopup="dialog"
      variant="secondary"
      size="icon-sm"
      onclick={onToggleFilters}
    >
      <FunnelIcon class="size-4.5" aria-hidden="true" />
    </Button>

    {#if hasFilterIndicator}
      <span
        class={cn(
          'pointer-events-none absolute -top-0.5 -right-0.5 size-2 rounded-full',
          'bg-accent ring-2 ring-background',
        )}
        aria-hidden="true"
      ></span>
    {/if}
  </div>
</div>
