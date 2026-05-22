<script lang="ts">
  import { Badge } from '@shared/ui';

  import type { DashboardStats } from '../model/dashboard-stats';

  type Props = {
    stats: DashboardStats;
  };

  type DashboardBadgeVariant = 'outline' | 'secondary';

  type DashboardBadge = {
    id: string;
    label: string;
    variant: DashboardBadgeVariant;
  };

  const SUMMARY_LABEL = 'Dashboard summary';

  let { stats }: Props = $props();

  const formatCountLabel = (count: number, singular: string, plural = `${singular}s`): string => {
    return `${count} ${count === 1 ? singular : plural}`;
  };

  const badges = $derived.by<DashboardBadge[]>(() => {
    const baseBadges: DashboardBadge[] = [
      {
        id: 'games',
        label: formatCountLabel(stats.games, 'game'),
        variant: 'outline',
      },
      {
        id: 'updates',
        label: formatCountLabel(stats.updates, 'update'),
        variant: 'outline',
      },
    ];

    if (stats.rollbacksReady <= 0) {
      return baseBadges;
    }

    return [
      ...baseBadges,
      {
        id: 'rollbacks-ready',
        label: `${stats.rollbacksReady} rollback-ready`,
        variant: 'secondary',
      },
    ];
  });
</script>

<div class="flex flex-wrap gap-1.5" aria-label={SUMMARY_LABEL}>
  {#each badges as badge (badge.id)}
    <Badge variant={badge.variant}>
      {badge.label}
    </Badge>
  {/each}
</div>
