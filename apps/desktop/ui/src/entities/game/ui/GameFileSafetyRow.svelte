<script lang="ts">
  import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
  import { Alert, AlertDescription } from '@shared/ui';
  import type { GameFileSafetyAssessment } from '../model/types';
  import { presentFileSafetyMessage } from '../model/file-safety-presenters';

  type Props = {
    assessment?: GameFileSafetyAssessment | null;
  };
  let { assessment = null }: Props = $props();

  const message = $derived(presentFileSafetyMessage(assessment));
</script>

<Alert size="sm" data-file-safety-row role="note">
  <ShieldAlertIcon aria-hidden="true" />
  <AlertDescription>{message}</AlertDescription>
</Alert>
