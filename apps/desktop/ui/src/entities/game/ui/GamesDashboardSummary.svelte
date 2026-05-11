<script lang="ts">
  import type { DashboardStats } from '../model/dashboard-stats';
  import { Badge, BadgeGroup } from '@shared/ui';

  type Props = {
    stats: DashboardStats;
  };

  let { stats }: Props = $props();
</script>

<BadgeGroup class="dashboard-summary" aria-label="Dashboard summary">
  <Badge pill surface="outline">{stats.games} games</Badge>
  <Badge pill surface="outline">{stats.updates} updates</Badge>

  {#if stats.backupsReady > 0}
    <Badge pill surface="outline" tone="success">{stats.backupsReady} backup-ready</Badge>
  {/if}
</BadgeGroup>
