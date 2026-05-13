<script lang="ts">
  import { GamesDashboardSummary, type DashboardStats } from '@entities/game';
  import { Button, Spinner } from '@shared/ui';
  import { cn } from '@shared/classnames';

  type ActionHandler = () => void;

  const DEFAULT_SCAN_BUTTON_LABEL = 'Scan Folder';

  const createDefaultDashboardStats = (): DashboardStats => ({
    games: 0,
    updates: 0,
    backupsReady: 0,
  });

  type Props = {
    hasGames?: boolean;
    busy?: boolean;
    scanButtonLabel?: string;
    dashboardStats?: DashboardStats;
    onRefresh?: ActionHandler;
    onScan?: ActionHandler;
  };

  const {
    hasGames = false,
    busy = false,
    scanButtonLabel = DEFAULT_SCAN_BUTTON_LABEL,
    dashboardStats = createDefaultDashboardStats(),
    onRefresh = () => undefined,
    onScan = () => undefined,
  }: Props = $props();

  const normalizedScanButtonLabel = $derived(scanButtonLabel.trim() || DEFAULT_SCAN_BUTTON_LABEL);
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
    aria-label="Library actions"
  >
    <Button variant="secondary" size="sm" disabled={busy} onclick={onRefresh}>
      {#if busy}
        <Spinner />
      {/if}
      Refresh Libraries
    </Button>

    <Button variant="default" size="sm" disabled={busy} onclick={onScan}>
      {#if busy}
        <Spinner />
      {/if}
      {normalizedScanButtonLabel}
    </Button>
  </div>
</div>
