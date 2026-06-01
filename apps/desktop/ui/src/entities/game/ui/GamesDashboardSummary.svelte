<script lang="ts">
  import { Badge } from '@shared/ui';
  import { t } from '@shared/i18n';

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

  let { stats }: Props = $props();

  const badges = $derived.by<DashboardBadge[]>(() => {
    const baseBadges: DashboardBadge[] = [
      {
        id: 'games',
        label: t('game.dashboard.games', { count: stats.games }),
        variant: 'outline',
      },
      {
        id: 'updates',
        label: t('game.dashboard.updates', { count: stats.updates }),
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
        label: t('game.dashboard.rollbacksReady', { count: stats.rollbacksReady }),
        variant: 'secondary',
      },
    ];
  });
</script>

<div class="flex flex-wrap gap-1.5" aria-label={t('game.dashboard.summary')}>
  {#each badges as badge (badge.id)}
    <Badge variant={badge.variant}>
      {badge.label}
    </Badge>
  {/each}
</div>
