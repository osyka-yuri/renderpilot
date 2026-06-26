<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Alert, AlertDescription } from '@shared/ui';
  import InfoIcon from '@lucide/svelte/icons/info';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import MonitorCheckIcon from '@lucide/svelte/icons/monitor-check';
  import CircleSlashIcon from '@lucide/svelte/icons/circle-slash';

  // A friendly terminal-state message: a single Alert with a leading icon, used
  // for the non-installable outcomes (native HDR, blacklisted, incompatible,
  // unsupported) and the load-error case. `tone` picks the Alert variant; `icon`
  // picks the leading glyph; `actions` is an optional trailing snippet (e.g. a
  // Retry button).
  type Props = {
    tone?: 'default' | 'warning';
    icon?: 'info' | 'warning' | 'hdr' | 'unsupported';
    message: string;
    actions?: Snippet;
  };

  const { tone = 'default', icon = 'info', message, actions }: Props = $props();
</script>

<Alert variant={tone} size="sm">
  {#if icon === 'hdr'}
    <MonitorCheckIcon aria-hidden="true" />
  {:else if icon === 'warning'}
    <TriangleAlertIcon aria-hidden="true" />
  {:else if icon === 'unsupported'}
    <CircleSlashIcon aria-hidden="true" />
  {:else}
    <InfoIcon aria-hidden="true" />
  {/if}
  <AlertDescription>{message}</AlertDescription>
  {#if actions}
    <div class="col-start-2 mt-2 flex items-center gap-2">
      {@render actions()}
    </div>
  {/if}
</Alert>
