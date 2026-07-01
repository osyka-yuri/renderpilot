<script lang="ts">
  import { Badge } from '@shared/ui';
  import { t, type MessageKey } from '@shared/i18n';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import FlaskConicalIcon from '@lucide/svelte/icons/flask-conical';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import type { MatchConfidence } from '../model/types';
  import RenoDxFieldLabel from './RenoDxFieldLabel.svelte';

  type Props = {
    confidence: MatchConfidence;
  };

  type ConfidenceView = {
    label: MessageKey;
    tint: string;
    Icon: typeof CircleCheckIcon;
  };

  const CONFIDENCE_VIEW = {
    verified: {
      label: 'gameDetails.renodx.confidenceVerified',
      tint: 'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
      Icon: CircleCheckIcon,
    },
    experimental: {
      label: 'gameDetails.renodx.confidenceExperimental',
      tint: 'border-transparent bg-amber-500/10 text-amber-700 dark:text-amber-400',
      Icon: FlaskConicalIcon,
    },
    untested: {
      label: 'gameDetails.renodx.confidenceUntested',
      tint: 'border-border bg-muted/50 text-muted-foreground',
      Icon: CircleHelpIcon,
    },
  } satisfies Record<MatchConfidence, ConfidenceView>;

  let { confidence }: Props = $props();

  const view = $derived(CONFIDENCE_VIEW[confidence]);
</script>

<RenoDxFieldLabel label={t('gameDetails.renodx.confidenceLabel')}>
  <Badge variant="outline" class={view.tint}>
    <view.Icon aria-hidden={true} />
    {t(view.label)}
  </Badge>
</RenoDxFieldLabel>
