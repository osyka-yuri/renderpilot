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
  import { t } from '@shared/i18n';

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
      <EmptyTitle>{t('games.filterEmpty.title')}</EmptyTitle>
      <EmptyDescription>
        {t('games.filterEmpty.description')}
      </EmptyDescription>
    </EmptyHeader>
    <EmptyContent>
      <Button variant="outline" size="sm" onclick={onResetFilters}>
        {t('games.filterEmpty.reset')}
      </Button>
    </EmptyContent>
  </Empty>
</div>
