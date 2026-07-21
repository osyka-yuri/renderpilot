<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { presentComponentFiles } from '@entities/component';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import {
    Badge,
    Button,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
    DownloadProgressBar,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { formatReleaseVersionLabel } from '../model/release-version-label';
  import { installedSelectionValue } from '../model/version-selection';

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, isDownloaded: boolean) => void;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();

  const filePresentation = $derived(presentComponentFiles(component));
  const fileName = $derived(filePresentation?.label ?? t('common.unknown'));
  const fileCount = $derived(filePresentation?.fileCount ?? 0);
  const fileLocations = $derived(filePresentation?.locations ?? []);
  const candidates = $derived(group?.candidates ?? []);

  const currentVersion = $derived(
    group?.version_report.kind === 'known' ? group.version_report.version : undefined,
  );

  const currentValue = $derived(
    installedSelectionValue(
      component.id,
      candidates.map((candidate) => candidate.artifact_id),
    ),
  );

  const currentLabel = $derived(
    formatReleaseVersionLabel({
      version: currentVersion,
      releaseLabel:
        group?.version_report.kind === 'known' ? group.version_report.release_label : null,
      isDebug: false,
      unknownLabel: t('common.unknown'),
    }),
  );

  // Track which artifact id the user actually clicked to download so the
  // progress bar appears only on the initiating control.
  let pendingArtifactId = $state<string | null>(null);

  // The dropdown always marks the backend-reported installed state as selected.
  // Its sentinel cannot collide with an artifact id, including a candidate that
  // happens to share the primary file hash with the installed bundle.

  // Bound selection, re-pinned to the installed version whenever an operation
  // settles (`busy` → false). This keeps the highlight correct even when a swap
  // FAILS — a clicked-but-never-installed version cannot stay selected. Also
  // resets pendingArtifactId.
  let selected = $state<string | undefined>(undefined);
  $effect(() => {
    if (!busy) {
      selected = currentValue;
      pendingArtifactId = null;
    }
  });

  function handleSwapChange(value: string | undefined) {
    if (!value || value === currentValue || busy) {
      return;
    }
    const candidate = candidates.find((c) => c.artifact_id === value);
    if (candidate) {
      pendingArtifactId = value;
      onSwap(component.id, value, candidate.is_downloaded);
    }
  }

  function handleRollback() {
    if (busy) {
      return;
    }
    onRollback(component.id);
  }

  const progressIds = $derived(pendingArtifactId ? [pendingArtifactId] : []);
</script>

<Item size="sm">
  <ItemContent>
    <ItemTitle>
      <span>{fileName}</span>
      {#if fileCount > 1}
        <Badge variant="outline" class="font-normal text-muted-foreground">
          {t('gameDetails.version.fileCount', { count: fileCount })}
        </Badge>
      {/if}
    </ItemTitle>
    <ItemDescription>
      {#if fileLocations.length === 0}
        <span>{t('common.unknown')}</span>
      {:else}
        {#each fileLocations as location (location)}
          <span class="block break-all">{location}</span>
        {/each}
      {/if}
    </ItemDescription>
  </ItemContent>
  <ItemActions>
    {#if candidates.length === 0}
      <span class="text-xs text-muted-foreground">{t('gameDetails.version.noReplacements')}</span>
    {:else}
      <DownloadProgressBar ids={progressIds} active={busy} />
      <Select type="single" bind:value={selected} disabled={busy} onValueChange={handleSwapChange}>
        <SelectTrigger size="sm" class="w-60">
          <span class="truncate">{currentLabel}</span>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={currentValue} label={currentLabel}>{currentLabel}</SelectItem>
          {#each candidates as candidate (candidate.artifact_id)}
            {@const isDebug = candidate.is_debug}
            {@const versionLabel = formatReleaseVersionLabel({
              version: candidate.version,
              releaseLabel: candidate.release_label,
              isDebug,
              unknownLabel: t('common.unknown'),
            })}
            <SelectItem value={candidate.artifact_id} label={versionLabel}>
              {#snippet children(snippetProps: { selected: boolean })}
                <span class="truncate pr-6">{versionLabel}</span>
                {#if !candidate.is_downloaded && !snippetProps.selected}
                  <span
                    class="pointer-events-none absolute inset-e-2 flex size-3.5 items-center justify-center text-muted-foreground"
                  >
                    <DownloadIcon class="size-4" aria-hidden="true" />
                  </span>
                {/if}
              {/snippet}
            </SelectItem>
          {/each}
        </SelectContent>
      </Select>
      {#if component.rollback_available}
        <Tooltip>
          <TooltipTrigger>
            <Button
              variant="ghost"
              size="icon-sm"
              disabled={busy}
              onclick={handleRollback}
              aria-label={t('gameDetails.version.restoreOriginal', { fileName })}
            >
              <Undo2Icon class="size-4" aria-hidden="true" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t('gameDetails.version.restoreOriginal', { fileName })}</TooltipContent>
        </Tooltip>
      {/if}
    {/if}
  </ItemActions>
</Item>
