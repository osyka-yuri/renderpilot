<script lang="ts">
  import type { DashboardStats } from '@features/games/games-screen-model';
  import Badge from '@shared/ui/Badge.svelte';
  import BadgeGroup from '@shared/ui/BadgeGroup.svelte';

  export let stats: DashboardStats;
</script>

<BadgeGroup class="dashboard-summary" aria-label="Dashboard summary">
  <Badge pill surface="outline">{stats.games} games</Badge>
  <Badge pill surface="outline">{stats.updates} updates</Badge>

  {#if stats.backupsReady > 0}
    <Badge pill surface="outline" tone="success">{stats.backupsReady} backup-ready</Badge>
  {/if}
</BadgeGroup>
