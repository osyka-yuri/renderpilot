<script lang="ts">
  import type { GameCandidate } from '@entities/game';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import { SelectItem } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { formatReleaseVersionLabel } from '../model/release-version-label';

  const { candidate }: { candidate: GameCandidate } = $props();

  const versionLabel = $derived(
    formatReleaseVersionLabel({
      version: candidate.catalog_package?.release.version ?? candidate.technical_version,
      releaseLabel: candidate.catalog_package?.release.label ?? candidate.release_label,
      isDebug: candidate.is_debug,
      unknownLabel: t('common.unknown'),
    }),
  );
</script>

<SelectItem
  value={candidate.artifact_id}
  label={versionLabel}
  disabled={candidate.d3d12_executable_action?.kind === 'repair_required'}
>
  {#snippet children(snippetProps: { selected: boolean })}
    <span class="min-w-0 flex-1 truncate">{versionLabel}</span>
    {#if !candidate.is_downloaded && !snippetProps.selected}
      <span
        class="pointer-events-none absolute inset-e-2 flex size-3.5 items-center justify-center text-muted-foreground"
      >
        <DownloadIcon class="size-4" aria-hidden="true" />
      </span>
    {/if}
  {/snippet}
</SelectItem>
