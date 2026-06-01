<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import { fileNameFromPath } from '@shared/path';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
    Alert,
    AlertDescription,
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
  import ComponentVersionRow from './ComponentVersionRow.svelte';
  import { buildStreamlineVersionModel, type BulkSwapItem } from '../model/streamline-versions';

  type Props = {
    components: GameGraphicsComponent[];
    groupsById: Record<string, GameCandidateGroup | null>;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
    onBulkSwap: (items: BulkSwapItem[]) => void;
    onBulkRollback: (componentIds: string[]) => void;
  };

  const { components, groupsById, busy, onSwap, onRollback, onBulkSwap, onBulkRollback }: Props =
    $props();

  // Sort by file name so the row order is stable across re-renders and matches
  // NVIDIA's canonical plugin ordering (sl.common, sl.dlss, sl.dlss_g, ...).
  const orderedComponents = $derived(
    [...components].sort((a, b) => {
      const aName = fileNameFromPath(a.files[0]?.path ?? '');
      const bName = fileNameFromPath(b.files[0]?.path ?? '');
      return aName.localeCompare(bName);
    }),
  );

  const versionModel = $derived(buildStreamlineVersionModel(components, groupsById));

  const triggerLabel = $derived(
    versionModel.currentVersion
      ? `v${versionModel.currentVersion}`
      : versionModel.isMixed
        ? 'Mixed versions'
        : 'Unknown',
  );

  function handleBulkChange(value: string | undefined) {
    if (!value || busy) return;
    const option = versionModel.options.find((candidate) => candidate.version === value);
    if (option) {
      onBulkSwap(option.items);
    }
  }

  // Plugins RenderPilot has swapped at least once keep a restorable `.bak` original.
  const rollbackIds = $derived(
    components.filter((component) => component.rollback_available).map((component) => component.id),
  );

  function handleRestoreAll() {
    if (busy || rollbackIds.length === 0) return;
    onBulkRollback(rollbackIds);
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <CardTitle>NVIDIA Streamline</CardTitle>
    <CardDescription>
      Multi-plugin framework. Every plugin must run the same version — set them all together here.
    </CardDescription>
  </CardHeader>

  <CardContent class="grid gap-3">
    <!-- Primary: safe bundle swap — one version across every plugin -->
    <Item size="sm" variant="outline" class="rounded-md bg-muted/30">
      <ItemContent>
        <ItemTitle>Streamline version · all plugins</ItemTitle>
        <ItemDescription>Applies one version across every plugin at once.</ItemDescription>
      </ItemContent>
      <ItemActions>
        {#if versionModel.options.length === 0}
          <span class="text-xs text-muted-foreground">No other versions</span>
        {:else}
          <Select type="single" disabled={busy} onValueChange={handleBulkChange}>
            <SelectTrigger size="sm" class="w-60">
              <span class="truncate">{triggerLabel}</span>
            </SelectTrigger>
            <SelectContent>
              {#each versionModel.options as option (option.version)}
                <SelectItem value={option.version} label={option.label}>
                  <span class="flex w-full items-center justify-between gap-2">
                    <span class="flex items-center gap-2">
                      {option.label}
                      {#if !option.allDownloaded}
                        <DownloadIcon class="size-4 text-muted-foreground" aria-hidden="true" />
                      {/if}
                    </span>
                    {#if !option.isComplete}
                      <span class="text-xs text-muted-foreground"
                        >updates {option.updateCount} · {option.missingCount} unavailable</span
                      >
                    {/if}
                  </span>
                </SelectItem>
              {/each}
            </SelectContent>
          </Select>
        {/if}
        {#if rollbackIds.length > 0}
          <Tooltip>
            <TooltipTrigger>
              <Button
                variant="ghost"
                size="icon-sm"
                disabled={busy}
                onclick={handleRestoreAll}
                aria-label="Restore all plugins to their original versions"
              >
                <Undo2Icon class="size-4" aria-hidden="true" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Restore all plugins to original</TooltipContent>
          </Tooltip>
        {/if}
      </ItemActions>
    </Item>

    {#if versionModel.isMixed}
      <Alert variant="warning" size="sm" role="note">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>
          Plugins are on different versions — choose one above to bring them back in sync.
        </AlertDescription>
      </Alert>
    {/if}

    <!-- Advanced: per-plugin overrides — single-file swaps can desync Streamline -->
    <Accordion type="single">
      <AccordionItem value="per-plugin" class="border-b-0">
        <AccordionTrigger class="text-muted-foreground">
          Advanced — per-plugin ({orderedComponents.length})
        </AccordionTrigger>
        <AccordionContent class="grid gap-2">
          <Alert variant="warning" size="sm" role="note">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertDescription>
              Changing one plugin on its own can desync Streamline. Prefer the version selector
              above unless a specific plugin needs a different build.
            </AlertDescription>
          </Alert>
          <ItemGroup class="rounded-md border bg-muted/30">
            {#each orderedComponents as component, index (component.id)}
              {@const group = groupsById[component.id] ?? null}

              {#if index > 0}
                <ItemSeparator />
              {/if}

              <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
            {/each}
          </ItemGroup>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  </CardContent>
</Card>
