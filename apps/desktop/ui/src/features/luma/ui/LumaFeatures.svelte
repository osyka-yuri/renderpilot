<script lang="ts">
  import { Badge } from '@shared/ui';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';

  import type { LumaFeatures as LumaFeaturesModel, LumaFeatureStatus } from '../model/types';

  type Props = {
    features: LumaFeaturesModel | null;
  };

  const { features }: Props = $props();

  const STATUS_KEY: Record<LumaFeatureStatus, MessageKeyWithoutParams> = {
    supported: 'gameDetails.luma.features.supported',
    unsupported: 'gameDetails.luma.features.unsupported',
    experimental: 'gameDetails.luma.features.experimental',
    unknown: 'gameDetails.luma.features.unknown',
  };

  function statusLabel(status: LumaFeatureStatus): string {
    return t(STATUS_KEY[status]);
  }
</script>

{#if features}
  <section class="space-y-2">
    <p class="text-sm font-medium">{t('gameDetails.luma.features.title')}</p>
    <div class="flex flex-wrap gap-2 text-xs">
      <Badge variant="outline"
        >{t('gameDetails.luma.features.dlssFsr')}: {statusLabel(features.dlss_fsr)}</Badge
      >
      <Badge variant="outline"
        >{t('gameDetails.luma.features.hdr')}: {statusLabel(features.hdr)}</Badge
      >
    </div>
  </section>
{/if}
