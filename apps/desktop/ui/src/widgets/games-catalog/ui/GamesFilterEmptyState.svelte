<script lang="ts">
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyMedia,
    EmptyTitle,
    LiveAnnouncer,
  } from '@shared/ui';
  import SearchIcon from '@lucide/svelte/icons/search';
  import { t } from '@shared/i18n';

  type Props = {
    onResetFilters?: () => void;
  };

  const { onResetFilters = () => undefined }: Props = $props();
</script>

<div class="flex flex-1 flex-col items-center justify-center">
  <LiveAnnouncer message={t('games.filterEmpty.title')} />
  <Empty class="border-0">
    <EmptyHeader>
      <EmptyMedia variant="icon">
        <SearchIcon />
      </EmptyMedia>
      <EmptyTitle level={2}>{t('games.filterEmpty.title')}</EmptyTitle>
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
