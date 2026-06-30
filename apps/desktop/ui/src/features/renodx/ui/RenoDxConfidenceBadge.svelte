<script lang="ts">
  import { Badge } from '@shared/ui';
  import { t, type MessageKey } from '@shared/i18n';
  import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
  import FlaskConicalIcon from '@lucide/svelte/icons/flask-conical';
  import CircleHelpIcon from '@lucide/svelte/icons/circle-help';

  import type { MatchConfidence } from '../model/types';
  import RenoDxFieldLabel from './RenoDxFieldLabel.svelte';

  const { confidence }: { confidence: MatchConfidence } = $props();

  const LABEL = {
    verified: 'gameDetails.renodx.confidenceVerified',
    experimental: 'gameDetails.renodx.confidenceExperimental',
    untested: 'gameDetails.renodx.confidenceUntested',
  } satisfies Record<MatchConfidence, MessageKey>;

  const TINT = {
    verified: 'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
    experimental: 'border-transparent bg-amber-500/10 text-amber-700 dark:text-amber-400',
    untested: 'border-border bg-muted/50 text-muted-foreground',
  } satisfies Record<MatchConfidence, string>;
</script>

<RenoDxFieldLabel label={t('gameDetails.renodx.confidenceLabel')}>
  <Badge variant="outline" class={TINT[confidence]}>
    {#if confidence === 'verified'}
      <CircleCheckIcon aria-hidden="true" />
    {:else if confidence === 'experimental'}
      <FlaskConicalIcon aria-hidden="true" />
    {:else}
      <CircleHelpIcon aria-hidden="true" />
    {/if}
    {t(LABEL[confidence])}
  </Badge>
</RenoDxFieldLabel>
