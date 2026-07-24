<script lang="ts">
  import type { D3d12ExecutableStatus } from '@entities/game';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { t } from '@shared/i18n';

  type Props = {
    status: D3d12ExecutableStatus;
  };

  const { status }: Props = $props();
</script>

{#if status.status === 'repair_required'}
  <div
    role="alert"
    class="mt-2 flex gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-2 text-xs text-destructive"
  >
    <TriangleAlertIcon class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
    <div class="grid min-w-0 gap-1">
      <span class="font-medium">{t('gameDetails.d3d12.status.repair')}</span>
      <span>{t('gameDetails.d3d12.repairGuidance')}</span>
      <span class="break-all text-foreground">{status.executable_path}</span>
      <span class="break-all text-muted-foreground">
        {t('gameDetails.d3d12.confirm.backup', { path: status.backup_path })}
      </span>
    </div>
  </div>
{:else}
  <span class="mt-1 block text-xs">
    {status.status === 'original'
      ? t('gameDetails.d3d12.status.original')
      : t('gameDetails.d3d12.status.patched', {
          from: status.original_sdk_version,
          to: status.current_sdk_version,
        })}
  </span>
  {#if status.selection_locked}
    <span class="block text-xs text-muted-foreground">
      {t('gameDetails.d3d12.executableLocked')}
    </span>
  {/if}
{/if}
