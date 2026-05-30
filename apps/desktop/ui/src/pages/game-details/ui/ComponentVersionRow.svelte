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

  type Props = {
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { component, group, busy, onSwap, onRollback }: Props = $props();

  const filePath = $derived(component.files[0]?.path ?? 'Unknown');
  const fileName = $derived(fileNameFromPath(filePath));
  const candidates = $derived(group?.candidates ?? []);

  function handleSwapChange(value: string | undefined) {
    if (!value || busy) return;
    const candidate = candidates.find((c) => c.artifact_id === value);
    onSwap(component.id, value, candidate?.manifest_entry_id ?? null);
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
      <span class="text-xs text-muted-foreground">No replacement versions</span>
    {:else}
      <Select type="single" disabled={busy} onValueChange={handleSwapChange}>
        <SelectTrigger size="sm" class="w-60">
          <span class="truncate">{group?.current_version ?? 'Unknown'}</span>
        </SelectTrigger>
        <SelectContent>
          {#each candidates as candidate (candidate.artifact_id)}
            {@const versionLabel = `v${candidate.version ?? 'Unknown'}`}
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
              aria-label={`Restore original ${fileName}`}
            >
              <Undo2Icon class="size-4" aria-hidden="true" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Restore original {fileName}</TooltipContent>
        </Tooltip>
      {/if}
    {/if}
  </ItemActions>
</Item>
