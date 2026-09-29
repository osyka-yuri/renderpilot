<script lang="ts">
  import { Badge } from '@shared/ui';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';
  import CircleXIcon from '@lucide/svelte/icons/circle-x';
  import FlaskConicalIcon from '@lucide/svelte/icons/flask-conical';

  import AddonFieldLabel from './AddonFieldLabel.svelte';
  import type { AddonBadgeTone } from './types';

  type Props = {
    /** Visual tone representing confidence or compatibility status. */
    tone: AddonBadgeTone;
    /** Already-translated field label (e.g. "Compatibility"). */
    fieldLabel: string;
    /** Already-translated status or confidence value (e.g. "Confirmed", "Unsupported"). */
    valueLabel: string;
  };

  type BadgeView = {
    tint: string;
    Icon: typeof CircleCheckIcon;
  };

  const BADGE_VIEW = {
    verified: {
      tint: 'border-success-border bg-success text-success-foreground',
      Icon: CircleCheckIcon,
    },
    experimental: {
      tint: 'border-warning-border bg-warning text-warning-foreground',
      Icon: FlaskConicalIcon,
    },
    untested: {
      tint: 'border-border bg-muted/50 text-muted-foreground',
      Icon: CircleHelpIcon,
    },
    unsupported: {
      tint: 'border-destructive/30 bg-destructive/10 text-destructive',
      Icon: CircleXIcon,
    },
  } satisfies Record<AddonBadgeTone, BadgeView>;

  let { tone, fieldLabel, valueLabel }: Props = $props();

  const view = $derived(BADGE_VIEW[tone]);
</script>

<AddonFieldLabel label={fieldLabel}>
  <Badge variant="outline" class={view.tint}>
    <view.Icon aria-hidden={true} />
    {valueLabel}
  </Badge>
</AddonFieldLabel>
