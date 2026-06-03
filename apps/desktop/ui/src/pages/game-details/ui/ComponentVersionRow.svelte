<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
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

  const filePath = $derived(component.files[0]?.path ?? t('common.unknown'));
  const fileName = $derived(fileNameFromPath(filePath));
  const candidates = $derived(group?.candidates ?? []);

  const currentLabel = $derived(
    group?.current_version ? `v${group.current_version}` : t('common.unknown'),
  );
  // The dropdown always marks the installed version as selected. Its value is the
  // installed file's content id, so after a swap/rollback the highlight follows the
  // new current version automatically — no stale pick lingers.
  const currentValue = $derived(component.files[0]?.sha256 ?? group?.current_version ?? 'current');

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
          <!-- Installed version: always the selected entry; selecting it is a no-op. -->
          <SelectItem value={currentValue} label={currentLabel}>{currentLabel}</SelectItem>
          {#each candidates as candidate (candidate.artifact_id)}
            {@const versionLabel = candidate.version
              ? `v${candidate.version}`
              : t('common.unknown')}
            <SelectItem value={candidate.artifact_id} label={versionLabel}>
              <span class="flex items-center gap-2">
                {versionLabel}
                {#if !candidate.is_downloaded}
                  <DownloadIcon class="size-4 text-muted-foreground" aria-hidden="true" />
                {/if}
              </span>
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
