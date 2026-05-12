<script lang="ts">
  import { GamesDashboardSummary, type DashboardStats } from '@entities/game';
  import { Button } from '@shared/ui';
  import { cn } from '@shared/utils';

  type ActionHandler = () => void;

  const DEFAULT_SCAN_BUTTON_LABEL = 'Scan Folder';

  const noop: ActionHandler = () => undefined;

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
    onRefresh = noop,
    onScan = noop,
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
    <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={onRefresh}>
      Refresh Libraries
    </Button>

    <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={onScan}>
      {normalizedScanButtonLabel}
    </Button>
  </div>
</div>
