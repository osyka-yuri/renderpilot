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
  // plugin at once. The model always includes the current version so the Select
  // has a stable SelectItem for it and never loses its selection after a swap.
  const versionModel = $derived(buildStreamlineVersionModel(components, groupsById));

  // Track which artifact ids the user clicked for bulk swap so the progress bar
  // appears only on the initiating control.
  let pendingArtifactIds = $state<string[]>([]);

  const triggerLabel = $derived(
    versionModel.currentVersion
      ? `v${versionModel.currentVersion}`
      : versionModel.isMixed
        ? t('gameDetails.streamline.mixed')
        : t('common.unknown'),
  );

  // The dropdown marks the common current version as selected; after a swap it
  // follows the new version automatically. When plugins are on mixed versions
  // there is no single current, so nothing is marked.
  const currentValue = $derived(versionModel.currentVersion ?? undefined);

  // Bound selection, re-pinned to the current version whenever an operation
  // settles (`busy` → false) so a FAILED bulk swap cannot leave a stale
  // highlight. Also resets pendingArtifactIds.
  let selected = $state<string | undefined>(undefined);
  $effect(() => {
    if (!busy) {
      selected = currentValue;
      pendingArtifactIds = [];
    }
  });

  function handleBulkChange(value: string | undefined) {
    if (!value || busy) {
      return;
    }
    const option = versionModel.options.find((o) => o.version === value);
    if (option && !option.isCurrent) {
      pendingArtifactIds = option.items.map((item) => item.artifactId);
      onBulkSwap(option.items);
    }
  }

  // True when there are no alternative versions to switch to: the only option
  // is the current version itself (or there are no options at all).
  const hasAlternatives = $derived(versionModel.options.some((o) => !o.isCurrent));

  // Plugins RenderPilot has swapped at least once keep a restorable `.bak` original.
  const rollbackIds = $derived(
    components.filter((component) => component.rollback_available).map((component) => component.id),
  );

  function handleRestoreAll() {
    if (busy || rollbackIds.length === 0) {
      return;
    }
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
        {#if !hasAlternatives}
          <span class="text-xs text-muted-foreground"
            >{t('gameDetails.streamline.noOtherVersions')}</span
          >
        {:else}
          <DownloadProgressBar ids={pendingArtifactIds} active={busy} />
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
              <!--
                All options come from a single loop so SelectItems are never
                remounted between rendering paths when the current version changes.
              -->
              {#each versionModel.options as option (option.version)}
                <SelectItem value={option.version} label={option.label}>
                  {#if option.isCurrent}
                    {option.label}
                  {:else}
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
                  {/if}
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
