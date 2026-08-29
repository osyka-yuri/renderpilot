<script lang="ts">
  import type {
    CoordinatedCandidateOption,
    GameCandidateGroup,
    GameLibraryComponent,
  } from '@entities/game';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
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
  import { buildStreamlineVersionModel } from '../model/streamline-versions';
  import type { SwapRequest } from '../model/swap-request';

  type Props = {
    components: GameLibraryComponent[];
    groupsById: Record<string, GameCandidateGroup | null>;
    coordinatedOptions: CoordinatedCandidateOption[];
    busy: boolean;
    onBulkSwap: (items: readonly SwapRequest[]) => void;
    onBulkRollback: (componentIds: string[]) => void;
  };

  const { components, groupsById, coordinatedOptions, busy, onBulkSwap, onBulkRollback }: Props =
    $props();

  // Streamline plugins are a matched set: one chosen version is applied to every
  // plugin at once. Only a known uniform release is selected; mixed and unknown
  // state stays descriptive in the trigger rather than becoming a fake option.
  const versionModel = $derived(
    buildStreamlineVersionModel(components, groupsById, coordinatedOptions),
  );

  // Track which artifact ids the user clicked for bulk swap so the progress bar
  // appears only on the initiating control.
  let pendingArtifactIds = $state<string[]>([]);

  const mixedLabel = $derived(
    versionModel.versionRange
      ? t('gameDetails.streamline.mixedRange', {
          min: versionModel.versionRange.min,
          max: versionModel.versionRange.max,
        })
      : t('gameDetails.streamline.mixed'),
  );

  const triggerLabel = $derived(
    versionModel.currentVersion
      ? `v${versionModel.currentVersion}`
      : versionModel.isMixed
        ? mixedLabel
        : t('common.unknown'),
  );

  // The trigger is descriptive only: every menu entry is a complete,
  // backend-coordinated replacement cohort.
  let selected = $state<string | undefined>(undefined);
  $effect(() => {
    if (!busy) {
      selected = undefined;
      pendingArtifactIds = [];
    }
  });

  function handleBulkChange(value: string | undefined) {
    if (!value || busy) {
      return;
    }
    const option = versionModel.options.find((o) => o.optionId === value);
    if (option) {
      pendingArtifactIds = option.items.map((item) => item.artifactId);
      onBulkSwap(option.items);
    }
  }

  const hasAlternatives = $derived(versionModel.options.length > 0);

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
    <CardTitle level={2}>NVIDIA Streamline</CardTitle>
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
              <span class="truncate text-foreground">{triggerLabel}</span>
            </SelectTrigger>
            <SelectContent>
              <!--
                All version options come from a single loop so SelectItems are
                never remounted between rendering paths when current changes.
              -->
              {#each versionModel.options as option (option.optionId)}
                <SelectItem value={option.optionId} label={option.label}>
                  <span class="flex w-full items-center justify-between gap-2">
                    <span class="flex items-center gap-2">
                      {option.label}
                      {#if !option.allDownloaded}
                        <DownloadIcon class="size-4 text-muted-foreground" aria-hidden="true" />
                      {/if}
                    </span>
                  </span>
                </SelectItem>
              {/each}
            </SelectContent>
          </Select>
        {/if}
        {#if rollbackIds.length > 0}
          <Tooltip>
            <TooltipTrigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="ghost"
                  size="icon-sm"
                  disabled={busy}
                  onclick={handleRestoreAll}
                  aria-label={t('gameDetails.streamline.restoreAllLabel')}
                >
                  <Undo2Icon class="size-4" aria-hidden="true" />
                </Button>
              {/snippet}
            </TooltipTrigger>
            <TooltipContent>{t('gameDetails.streamline.restoreAllTooltip')}</TooltipContent>
          </Tooltip>
        {/if}
      </ItemActions>
    </Item>
  </CardContent>
</Card>
