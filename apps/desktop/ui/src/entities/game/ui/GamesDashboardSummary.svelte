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
    return [
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
  });
</script>

<div class="flex flex-wrap gap-1.5" aria-label={t('game.dashboard.summary')}>
  {#each badges as badge (badge.id)}
    <Badge variant={badge.variant}>
      {badge.label}
    </Badge>
  {/each}
</div>
