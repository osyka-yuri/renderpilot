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
    scanButtonLabel?: string;
    dashboardStats?: DashboardStats;
    onScan?: ActionHandler;
  };

  const {
    hasGames = false,
    busy = false,
    scanButtonLabel = '',
    dashboardStats = createDefaultDashboardStats(),
    onScan = () => undefined,
  }: Props = $props();

  const normalizedScanButtonLabel = $derived(scanButtonLabel.trim() || t('games.scanFolder'));
</script>

<div
  class={cn(
    'flex min-w-0 flex-wrap items-center justify-between gap-3 gap-x-4 px-1',
    'max-md:items-start',
  )}
  aria-busy={busy ? 'true' : 'false'}
>
  {#if hasGames}
    <GamesDashboardSummary stats={dashboardStats} />
  {/if}

  <div
    class={cn('ml-auto flex flex-wrap justify-end gap-2', 'max-md:ml-0 max-md:justify-start')}
    role="group"
    aria-label={t('games.libraryActions')}
  >
    <Button variant="default" size="sm" disabled={busy} onclick={onScan}>
      {#if busy}
        <Spinner />
      {/if}
      {normalizedScanButtonLabel}
    </Button>
  </div>
</div>
