<script lang="ts">
  import { GamesDashboardSummary, type DashboardStats } from '@entities/game';
  import { Button, Spinner } from '@shared/ui';
  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';

  type ActionHandler = () => void;

  const createDefaultDashboardStats = (): DashboardStats => ({
    games: 0,
    updates: 0,
    rollbacksReady: 0,
  });

  type Props = {
    hasGames?: boolean;
    busy?: boolean;
    addGameButtonLabel?: string;
    dashboardStats?: DashboardStats;
    onAddGame?: ActionHandler;
  };

  const {
    hasGames = false,
    busy = false,
    addGameButtonLabel = '',
    dashboardStats = createDefaultDashboardStats(),
    onAddGame = () => undefined,
  }: Props = $props();

  const normalizedAddGameButtonLabel = $derived(addGameButtonLabel.trim() || t('games.addGame'));
</script>

<div
  class={cn(
    'flex min-w-0 flex-wrap items-center justify-between gap-3 gap-x-4 px-1',
    'max-md:items-start',
  )}
  aria-busy={busy}
>
  {#if hasGames}
    <GamesDashboardSummary stats={dashboardStats} />
  {/if}

  <div
    class={cn('ms-auto flex flex-wrap justify-end gap-2', 'max-md:ms-0 max-md:justify-start')}
    role="group"
    aria-label={t('games.libraryActions')}
  >
    <Button variant="default" size="sm" disabled={busy} onclick={onAddGame}>
      {#if busy}
        <Spinner />
      {/if}
      {normalizedAddGameButtonLabel}
    </Button>
  </div>
</div>
