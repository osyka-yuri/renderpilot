<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Alert, AlertDescription } from '@shared/ui';
  import InfoIcon from '@lucide/svelte/icons/info';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import MonitorCheckIcon from '@lucide/svelte/icons/monitor-check';
  import CircleSlashIcon from '@lucide/svelte/icons/circle-slash';

  type AlertTone = 'default' | 'warning';
  type AlertIcon = 'info' | 'warning' | 'hdr' | 'unsupported';

  type Props = {
    tone?: AlertTone;
    icon?: AlertIcon;
    message: string;
    actions?: Snippet;
  };

  const { tone = 'default', icon = 'info', message, actions }: Props = $props();

  const ICON_BY_TYPE = {
    info: InfoIcon,
    warning: TriangleAlertIcon,
    hdr: MonitorCheckIcon,
    unsupported: CircleSlashIcon,
  } satisfies Record<AlertIcon, typeof InfoIcon>;

  const Icon = $derived(ICON_BY_TYPE[icon]);
</script>

<Alert variant={tone} size="sm">
  <Icon aria-hidden="true" />

  <AlertDescription>{message}</AlertDescription>

  {#if actions}
    <div class="col-start-2 mt-2 flex items-center gap-2">
      {@render actions()}
    </div>
  {/if}
</Alert>
