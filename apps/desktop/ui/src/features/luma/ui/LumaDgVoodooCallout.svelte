<script lang="ts">
  import { AddonStateMessage } from '@entities/addon';
  import { t } from '@shared/i18n';

  import { dgVoodooRequirement } from '../model/external-requirements';
  import type { LumaManagedDependencySummary } from '../model/types';

  type Props = {
    requirement: LumaManagedDependencySummary | null;
  };

  const { requirement }: Props = $props();

  const dgvoodoo = $derived(dgVoodooRequirement(requirement));
</script>

{#if dgvoodoo}
  <AddonStateMessage
    message={t('gameDetails.luma.dgvoodoo.managed', { version: dgvoodoo.version })}
  />
{/if}
