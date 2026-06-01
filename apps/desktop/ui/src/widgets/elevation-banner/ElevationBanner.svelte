<script lang="ts">
  import { Button } from '@shared/ui';
  import { requestAdminRelaunch } from '@entities/app';
  import { describeCommandErrorTechnical } from '@shared/api';
  import { publishErrorNotification } from '@shared/notifications';
  import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
  import XIcon from '@lucide/svelte/icons/x';

  type Props = {
    /**
     * Whether the surrounding app is running with admin rights. The banner
     * renders only when this is `false`.
     */
    isElevated: boolean;
    /** `false` on non-Windows; the banner is hidden in that case. */
    elevationSupported: boolean;
  };

  const { isElevated, elevationSupported }: Props = $props();

  // Session-only: dismissal does NOT persist between launches. Users who
  // want a permanent fix should set "Run as administrator" in Windows
  // Explorer (right-click .exe -> Properties -> Compatibility).
  let dismissed = $state(false);
  let busy = $state(false);

  const visible = $derived(elevationSupported && !isElevated && !dismissed);

  async function handleRelaunch() {
    if (busy) return;
    busy = true;
    try {
      // On success the process exits and this promise never resolves; on
      // failure (cancel UAC, OS policy) it rejects with a CommandError.
      await requestAdminRelaunch();
    } catch (e) {
      publishErrorNotification(
        'Could not relaunch as administrator',
        describeCommandErrorTechnical(e),
      );
    } finally {
      busy = false;
    }
  }
</script>

{#if visible}
  <div
    role="alert"
    class="mx-4 my-2 flex items-center gap-3 rounded-md border border-warning/40 bg-warning/10 px-4 py-3"
  >
    <ShieldAlertIcon class="size-5 shrink-0 text-warning" aria-hidden="true" />
    <div class="grid min-w-0 flex-1 gap-1">
      <div class="text-sm font-medium text-foreground">Administrator privileges required</div>
      <div class="text-xs text-muted-foreground">
        RenderPilot is running without administrator rights. NVIDIA driver settings (DLSS render
        preset) cannot be changed in this session.
      </div>
    </div>
    <div class="flex items-center gap-2">
      <Button variant="default" size="sm" disabled={busy} onclick={handleRelaunch}>
        Relaunch as administrator
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Dismiss for this session"
        onclick={() => (dismissed = true)}
      >
        <XIcon class="size-4" aria-hidden="true" />
      </Button>
    </div>
  </div>
{/if}
