<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import {
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
  import { DownloadProgressBar } from '@entities/library';
  import { buildStreamlineVersionModel, type BulkSwapItem } from '../model/streamline-versions';

  type Props = {
    components: GameGraphicsComponent[];
    groupsById: Record<string, GameCandidateGroup | null>;
    busy: boolean;
    onBulkSwap: (items: BulkSwapItem[]) => void;
    onBulkRollback: (componentIds: string[]) => void;
  };

  const { components, groupsById, busy, onBulkSwap, onBulkRollback }: Props = $props();

  // Streamline plugins are a matched set: one chosen version is applied to every
  // plugin at once. The model lists the versions that change at least one plugin.
  const versionModel = $derived(buildStreamlineVersionModel(components, groupsById));

  // Track which artifact ids the user clicked for bulk swap so the progress bar
  // appears only on the initiating control.
  let pendingArtifactIds = $state<string[]>([]);

  const currentLabel = $derived(
    versionModel.currentVersion ? `v${versionModel.currentVersion}` : '',
  );

  const triggerLabel = $derived(
    versionModel.currentVersion
      ? currentLabel
      : versionModel.isMixed
        ? t('gameDetails.streamline.mixed')
        : t('common.unknown'),
  );

  // The dropdown marks the common current version as selected; after a swap it
  // follows the new version automatically. When plugins are on mixed versions there
  // is no single current, so nothing is marked.
  const currentValue = $derived(versionModel.currentVersion ?? undefined);

  // Bound selection, re-pinned to the current version whenever an operation settles
  // (`busy` → false) so a FAILED bulk swap cannot leave a stale highlight.
  // Also resets pendingArtifactIds.
  let selected = $state<string | undefined>(undefined);
  $effect(() => {
    if (!busy) {
      selected = currentValue;
      pendingArtifactIds = [];
    }
  });

  function handleBulkChange(value: string | undefined) {
    if (!value || value === versionModel.currentVersion || busy) return;
    const option = versionModel.options.find((candidate) => candidate.version === value);
    if (option) {
      pendingArtifactIds = option.items.map((item) => item.artifactId);
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
      {t('gameDetails.streamline.description')}
    </CardDescription>
  </CardHeader>

  <CardContent class="grid gap-3">
    <!-- Safe bundle swap: one version applied across every plugin together. -->
    <Item size="sm" variant="outline" class="rounded-md bg-muted/30">
      <ItemContent>
        <ItemTitle>{t('gameDetails.streamline.versionTitle')}</ItemTitle>
        <ItemDescription>{t('gameDetails.streamline.versionDescription')}</ItemDescription>
      </ItemContent>
      <ItemActions>
        {#if versionModel.options.length === 0}
          <span class="text-xs text-muted-foreground"
            >{t('gameDetails.streamline.noOtherVersions')}</span
          >
        {:else}
          <DownloadProgressBar ids={pendingArtifactIds} active={busy} class="mr-2" />
          <Select
            type="single"
            bind:value={selected}
            disabled={busy}
            onValueChange={handleBulkChange}
          >
            <SelectTrigger size="sm" class="w-60">
              <span class="truncate">{triggerLabel}</span>
            </SelectTrigger>
            <SelectContent>
              {#if versionModel.currentVersion}
                <!-- Common current version: the selected entry; selecting it is a no-op. -->
                <SelectItem value={versionModel.currentVersion} label={currentLabel}>
                  {currentLabel}
                </SelectItem>
              {/if}
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
                      <span class="text-xs text-muted-foreground">
                        {t('gameDetails.streamline.updatesSummary', {
                          updates: option.updateCount,
                          missing: option.missingCount,
                        })}</span
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
                aria-label={t('gameDetails.streamline.restoreAllAria')}
              >
                <Undo2Icon class="size-4" aria-hidden="true" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t('gameDetails.streamline.restoreAllTooltip')}</TooltipContent>
          </Tooltip>
        {/if}
      </ItemActions>
    </Item>

    {#if versionModel.isMixed}
      <Alert variant="warning" size="sm" role="note">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>
          {t('gameDetails.streamline.mixedWarning')}
        </AlertDescription>
      </Alert>
    {/if}
  </CardContent>
</Card>
