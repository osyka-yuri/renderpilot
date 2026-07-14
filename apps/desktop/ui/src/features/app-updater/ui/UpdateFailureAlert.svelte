<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

  import { t } from '@shared/i18n';

  import {
    failureDescriptionKey,
    failureTitleKey,
    type UpdateFailureKind,
  } from '../model/dialog-view';

  type Props = {
    kind: UpdateFailureKind;
  };

  const { kind }: Props = $props();
</script>

<div
  role="alert"
  class="flex gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive"
>
  <TriangleAlertIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
  <div class="flex min-w-0 flex-col gap-1">
    <p class="font-medium">{t(failureTitleKey(kind))}</p>
    <p class="text-destructive/90">{t(failureDescriptionKey(kind))}</p>
  </div>
</div>
