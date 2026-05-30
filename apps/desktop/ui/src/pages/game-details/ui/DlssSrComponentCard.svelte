<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import {
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemSeparator,
    ItemTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import type { NvapiContext } from '../model/create-nvapi-context.svelte';
  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    gameId: string;
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    nvapi: NvapiContext;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { gameId, component, group, nvapi, busy, onSwap, onRollback }: Props = $props();

  // ── derived from props ───────────────────────────────────────────
  // NVAPI is "unavailable" when we tried to load and got an error.
  // Differs from "still loading" (busy + no snapshot yet).
  const nvapiUnavailable = $derived(nvapi.loadError !== null && !nvapi.busy);
  const presetLoading = $derived(nvapi.busy && !nvapi.hasSnapshot);
  const presetReady = $derived(nvapi.hasSnapshot && nvapi.hasProfile && !nvapiUnavailable);
  // `!nvapi.canWrite` disables the preset/revert controls when the process
  // is not running elevated. NVAPI writes require admin; reads are fine,
  // so the row still renders the current preset, just non-interactively.
  const presetDisabled = $derived(busy || nvapi.busy || !presetReady || !nvapi.canWrite);

  function handlePresetChange(value: string | undefined) {
    if (!value || presetDisabled) return;
    void nvapi.setPresetValue(gameId, value);
  }

  function handleRevertPredefined() {
    if (presetDisabled) return;
    void nvapi.revertPreset(gameId, 'predefined');
  }

  function handleRevertBaseline() {
    if (presetDisabled) return;
    void nvapi.revertPreset(gameId, 'baseline');
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex items-start justify-between gap-3">
      <div class="grid min-w-0 gap-1">
        <CardTitle>NVIDIA DLSS Super Resolution</CardTitle>
        <CardDescription>Driver-level override applied via NVAPI.</CardDescription>
      </div>
      {#if nvapi.dllInfo}
        <div class="shrink-0 text-right text-xs text-muted-foreground">
          <div class="font-medium text-foreground">
            {nvapi.dllInfo.manifest_label ?? `DLSS ${nvapi.dllInfo.version}`}
          </div>
          <div>v{nvapi.dllInfo.version}</div>
        </div>
      {/if}
    </div>
  </CardHeader>

  <CardContent class="grid gap-3">
    <!-- DLL version + render preset rows in one ItemGroup -->
    <ItemGroup class="rounded-md border bg-muted/30">
      <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />

      <ItemSeparator />

      <!-- Render preset (NVAPI) row -->
      <Item size="sm">
        <ItemContent>
          <ItemTitle>Render preset</ItemTitle>
          <ItemDescription>
            {#if nvapiUnavailable}
              NVAPI unavailable on this system.
            {:else if presetLoading}
              Loading driver state…
            {:else if !nvapi.canWrite}
              Administrator privileges required to change this setting.
            {:else if nvapi.hasSnapshot && !nvapi.hasProfile}
              No driver profile yet — launch the game once first.
            {:else}
              Driver-level DLSS preset override applied via NVAPI.
            {/if}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          {#if nvapiUnavailable}
            <span class="text-xs text-muted-foreground italic">unavailable</span>
          {:else}
            <Select type="single" disabled={presetDisabled} onValueChange={handlePresetChange}>
              <SelectTrigger size="sm" class="w-60">
                {#if presetLoading}
                  <span class="text-muted-foreground">Loading…</span>
                {:else if nvapi.snapshot}
                  <span class="truncate">{nvapi.snapshot.current.label}</span>
                {:else}
                  <span class="text-muted-foreground">—</span>
                {/if}
              </SelectTrigger>
              <SelectContent>
                {#each nvapi.orderedValues as option (option.wire)}
                  <SelectItem value={option.wire} label={option.label} disabled={!option.supported}>
                    <span class="flex w-full items-center justify-between gap-2">
                      <span>{option.label}</span>
                      {#if !option.supported}
                        <span class="text-xs text-muted-foreground">unsupported</span>
                      {/if}
                    </span>
                  </SelectItem>
                {/each}
              </SelectContent>
            </Select>

            <Tooltip>
              <TooltipTrigger>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  disabled={presetDisabled}
                  onclick={handleRevertPredefined}
                  aria-label="Revert to driver default"
                >
                  <Undo2Icon class="size-4" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Revert to driver default</TooltipContent>
            </Tooltip>

            {#if nvapi.baseline}
              <Tooltip>
                <TooltipTrigger>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    disabled={presetDisabled}
                    onclick={handleRevertBaseline}
                    aria-label="Revert to baseline captured before RenderPilot's first write"
                  >
                    <HistoryIcon class="size-4" aria-hidden="true" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Revert to baseline (pre-RenderPilot value)</TooltipContent>
              </Tooltip>
            {/if}
          {/if}
        </ItemActions>
      </Item>
    </ItemGroup>
  </CardContent>
</Card>
