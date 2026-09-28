<script lang="ts">
  import { presentDetectedEngines, type GameFileSafetyAssessment } from '@entities/game';
  import { Alert, AlertDescription, AlertTitle } from '@shared/ui';
  import { t } from '@shared/i18n';

  let { assessment }: { assessment: GameFileSafetyAssessment } = $props();
  const engines = $derived(presentDetectedEngines(assessment.detected_engines));
</script>

{#if engines.length > 0}
  <Alert variant="default" size="sm" role="note">
    <AlertTitle>{t('gameDetails.fileSafety.detectedEngines')}</AlertTitle>
    <AlertDescription>
      <ul class="flex flex-wrap gap-x-3 gap-y-1 font-medium">
        {#each engines as engine (engine)}
          <li>{engine}</li>
        {/each}
      </ul>
    </AlertDescription>
  </Alert>
{/if}
