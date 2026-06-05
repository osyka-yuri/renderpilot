<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { displayComponentFilePath } from '@entities/component';
  import { fileNameFromPath } from '@shared/path';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import {
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
  } from '@shared/ui';
  import { t } from '@shared/i18n';

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, isDownloaded: boolean) => void;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();

  const filePath = $derived(displayComponentFilePath(component) ?? t('common.unknown'));
  const fileName = $derived(fileNameFromPath(filePath));
  const candidates = $derived(group?.candidates ?? []);

  const currentHash = $derived(component.files[0]?.sha256);
  const currentVersion = $derived(group?.current_version);

  const currentCandidate = $derived(
    candidates.find((c) => currentHash && c.sha256 === currentHash),
  );
  const currentValue = $derived(
    currentCandidate?.artifact_id ?? currentHash ?? currentVersion ?? 'current',
  );
  const isCurrentDebug = $derived(currentCandidate?.is_debug ?? false);

  const currentLabel = $derived(
    group?.current_version
      ? `v${group.current_version}${isCurrentDebug ? ' (Debug)' : ''}`
      : t('common.unknown'),
  );
  // The dropdown always marks the installed version as selected. Its value is the
  // installed file's content id, so after a swap/rollback the highlight follows the
  // new current version automatically — no stale pick lingers.

  // Bound selection, re-pinned to the installed version whenever an operation
  // settles (`busy` → false). This keeps the highlight correct even when a swap
  // FAILS — a clicked-but-never-installed version cannot stay selected.
  let selected = $state<string | undefined>(undefined);
  $effect(() => {
    if (!busy) {
      selected = currentValue;
    }
  });

  function handleSwapChange(value: string | undefined) {
    if (!value || value === currentValue || busy) return;
    const candidate = candidates.find((c) => c.artifact_id === value);
    if (candidate) {
      onSwap(component.id, value, candidate.is_downloaded);
    }
  }

  function handleRollback() {
    if (busy) return;
    onRollback(component.id);
  }
</script>

<Item size="sm">
  <ItemContent>
    <ItemTitle>{fileName}</ItemTitle>
    <ItemDescription>
      <span class="break-all">{filePath}</span>
    </ItemDescription>
  </ItemContent>
  <ItemActions>
    {#if candidates.length === 0}
      <span class="text-xs text-muted-foreground">{t('gameDetails.version.noReplacements')}</span>
    {:else}
      <Select type="single" bind:value={selected} disabled={busy} onValueChange={handleSwapChange}>
        <SelectTrigger size="sm" class="w-60">
          <span class="truncate">{currentLabel}</span>
        </SelectTrigger>
        <SelectContent>
          <!-- Installed version: only render if it's not a known candidate to avoid duplication -->
          {#if !currentCandidate}
            <SelectItem value={currentValue} label={currentLabel}>{currentLabel}</SelectItem>
          {/if}
          {#each candidates as candidate (candidate.artifact_id)}
            {@const isDebug = candidate.is_debug}
            {@const versionLabel = candidate.version
              ? `v${candidate.version}${isDebug ? ' (Debug)' : ''}`
              : t('common.unknown')}
            <SelectItem value={candidate.artifact_id} label={versionLabel}>
              {#snippet children(snippetProps)}
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
