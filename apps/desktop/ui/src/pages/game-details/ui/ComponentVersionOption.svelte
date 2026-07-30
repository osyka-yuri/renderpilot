<script lang="ts">
  import type { GameCandidate } from '@entities/game';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import { SelectItem } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { presentCatalogCandidateOption } from '../model/catalog-candidate-presentation';

  const { candidate }: { candidate: GameCandidate } = $props();

  const presentation = $derived(
    presentCatalogCandidateOption(candidate, {
      unknown: t('common.unknown'),
    }),
  );
</script>

<SelectItem
  value={candidate.artifact_id}
  label={presentation.versionLabel}
  class="items-start"
  disabled={candidate.d3d12_executable_action?.kind === 'repair_required'}
>
  {#snippet children(snippetProps: { selected: boolean })}
    <div class="flex min-w-0 flex-1 flex-col">
      <span class="truncate">{presentation.versionLabel}</span>
      {#if presentation.componentVersions.length > 0}
        <span class="truncate text-xs text-muted-foreground"
          >{presentation.componentVersions.join(' · ')}</span
        >
      {/if}
    </div>
    {#if !candidate.is_downloaded && !snippetProps.selected}
      <span
        class="pointer-events-none absolute inset-e-2 flex size-3.5 items-center justify-center text-muted-foreground"
      >
        <DownloadIcon class="size-4" aria-hidden="true" />
      </span>
    {/if}
  {/snippet}
</SelectItem>
