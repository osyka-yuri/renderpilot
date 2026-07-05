<script lang="ts">
  import { Badge } from '@shared/ui';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import FlaskConicalIcon from '@lucide/svelte/icons/flask-conical';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import AddonFieldLabel from './AddonFieldLabel.svelte';
  import type { AddonMatchConfidence } from './types';

  type Props = {
    confidence: AddonMatchConfidence;
    /** Already-translated field label (e.g. "RenoDX compatibility"). */
    fieldLabel: string;
    /** Already-translated confidence text (e.g. "Works"). */
    confidenceLabel: string;
  };

  type ConfidenceView = {
    tint: string;
    Icon: typeof CircleCheckIcon;
  };

  const CONFIDENCE_VIEW = {
    verified: {
      tint: 'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
      Icon: CircleCheckIcon,
    },
    experimental: {
      tint: 'border-transparent bg-amber-500/10 text-amber-700 dark:text-amber-400',
      Icon: FlaskConicalIcon,
    },
    untested: {
      tint: 'border-border bg-muted/50 text-muted-foreground',
      Icon: CircleHelpIcon,
    },
  } satisfies Record<AddonMatchConfidence, ConfidenceView>;

  let { confidence, fieldLabel, confidenceLabel }: Props = $props();

  const view = $derived(CONFIDENCE_VIEW[confidence]);
</script>

<AddonFieldLabel label={fieldLabel}>
  <Badge variant="outline" class={view.tint}>
    <view.Icon aria-hidden={true} />
    {confidenceLabel}
  </Badge>
</AddonFieldLabel>
