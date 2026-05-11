<script lang="ts">
  import { GamesDashboardSummary, type DashboardStats } from '@entities/game';
  import { Button } from '@shared/ui';

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

  let {
    hasGames = false,
    busy = false,
    scanButtonLabel = DEFAULT_SCAN_BUTTON_LABEL,
    dashboardStats = createDefaultDashboardStats(),
    onRefresh = noop,
    onScan = noop,
  }: Props = $props();

  const normalizedScanButtonLabel = $derived(scanButtonLabel.trim() || DEFAULT_SCAN_BUTTON_LABEL);
</script>

<div class="overview-bar" aria-busy={busy ? 'true' : 'false'}>
  {#if hasGames}
    <GamesDashboardSummary stats={dashboardStats} />
  {/if}

  <div class="action-group" role="group" aria-label="Library actions">
    <Button variant="secondary" size="sm" disabled={busy} loading={busy} onclick={onRefresh}>
      Refresh Libraries
    </Button>

    <Button variant="primary" size="sm" disabled={busy} loading={busy} onclick={onScan}>
      {normalizedScanButtonLabel}
    </Button>
  </div>
</div>

<style>
  .overview-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3) var(--space-4);
    min-width: 0;
    padding-inline: var(--space-1);
  }

  .action-group {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-left: auto;
  }

  @media (max-width: 760px) {
    .overview-bar {
      align-items: flex-start;
    }

    .action-group {
      justify-content: flex-start;
      margin-left: 0;
    }
  }
</style>
