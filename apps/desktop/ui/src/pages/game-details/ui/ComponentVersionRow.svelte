<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { GameCandidate, GameCandidateGroup, GameLibraryComponent } from '@entities/game';
  import { presentComponentFiles } from '@entities/component';
  import { isD3d12ExecutableMutationAction } from '@shared/model';
  import { publishCommandErrorNotification } from '@shared/notifications';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
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
    SelectGroup,
    SelectGroupHeading,
    SelectItem,
    SelectSeparator,
    SelectTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
    DownloadProgressBar,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { formatReleaseVersionLabel } from '../model/release-version-label';
  import { installedSelectionValue } from '../model/version-selection';
  import { createD3d12PreflightFlow } from '../model/create-d3d12-preflight-flow.svelte';
  import { requiresD3d12Preflight, type PreparedD3d12Swap } from '../model/d3d12-preflight';
  import { prepareD3d12Swap } from '../model/prepare-d3d12-operation';
  import { partitionD3d12Candidates } from '../model/candidate-partition';
  import type { SwapHandler } from '../model/create-game-details-page-model';
  import { d3d12PlanFingerprint, type PreparedSwapPresentation } from '../model/swap-request';
  import DeveloperModeRequirementDialog from './DeveloperModeRequirementDialog.svelte';
  import ComponentVersionOption from './ComponentVersionOption.svelte';
  import D3d12ExecutableStatusPanel from './D3d12ExecutableStatusPanel.svelte';

  type Props = {
    component: GameLibraryComponent;
    group: GameCandidateGroup | null;
    installPath: string;
    busy: boolean;
    onSwap: SwapHandler;
    onRollback: (componentId: string) => void;
  };

  type SwapOwner = {
    gameId: string;
    installPath: string;
    componentId: string;
    artifactId: string;
    planFingerprint: string;
  };

  type PendingD3d12Swap = {
    candidate: GameCandidate;
    owner: SwapOwner;
  };

  const { component, group, installPath, busy, onSwap, onRollback }: Props = $props();

  const filePresentation = $derived(presentComponentFiles(component));
  const fileName = $derived(filePresentation?.label ?? t('common.unknown'));
  const fileCount = $derived(filePresentation?.fileCount ?? 0);
  const fileLocations = $derived(filePresentation?.locations ?? []);
  const candidates = $derived(group?.candidates ?? []);
  const candidatePartition = $derived(partitionD3d12Candidates(candidates));

  const currentVersion = $derived(
    group?.version_report.kind === 'known'
      ? (group.version_report.catalog_release?.version ?? group.version_report.technical_version)
      : undefined,
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
        group?.version_report.kind === 'known'
          ? (group.version_report.catalog_release?.label ?? group.version_report.release_label)
          : null,
      unknownLabel: t('common.unknown'),
    }),
  );

  // Track which artifact id the user actually clicked to download so the
  // progress bar appears only on the initiating control.
  let pendingArtifactId = $state<string | null>(null);
  const preflight = createD3d12PreflightFlow<PendingD3d12Swap, PreparedD3d12Swap>({
    prepare: (pending) =>
      prepareD3d12Swap(
        pending.owner.gameId,
        pending.owner.componentId,
        pending.candidate.artifact_id,
      ),
    isCurrent: (pending) => isCurrentSwapOwner(pending.owner),
    onReady: (pending, prepared) => {
      handlePreparedSwap(pending, prepared);
    },
    onError: (error) => {
      selected = currentValue;
      publishCommandErrorNotification(error);
    },
    onCancel: () => {
      selected = currentValue;
    },
  });

  onDestroy(() => {
    invalidatePendingSwap();
  });

  // The dropdown always marks the backend-reported installed state as selected.
  // Its sentinel cannot collide with an artifact id, including a candidate that
  // happens to share the primary file hash with the installed bundle.

  // Bound selection, re-pinned to the installed version whenever an operation
  // settles (`busy` → false). This keeps the highlight correct even when a swap
  // FAILS — a clicked-but-never-installed version cannot stay selected. Also
  // resets pendingArtifactId.
  let selected = $state<string | undefined>(undefined);
  let preflightOwnerKey: string | null = null;
  $effect(() => {
    if (!busy) {
      selected = currentValue;
      pendingArtifactId = null;
    }
  });
  $effect(() => {
    const ownerKey = `${component.game_id}\0${component.id}`;
    if (preflightOwnerKey === null) {
      preflightOwnerKey = ownerKey;
      return;
    }
    if (ownerKey !== preflightOwnerKey) {
      preflightOwnerKey = ownerKey;
      invalidatePendingSwap();
    }
  });

  function handleSwapChange(value: string | undefined) {
    if (!value || value === currentValue || busy || preflight.planning) {
      return;
    }
    const candidate = candidates.find((c) => c.artifact_id === value);
    if (candidate) {
      const action = candidate.d3d12_executable_action;
      if (action?.kind === 'repair_required') {
        selected = currentValue;
        return;
      }
      if (requiresD3d12Preflight(component.technology)) {
        void preflight.start({ candidate, owner: currentSwapOwner(candidate.artifact_id) });
        return;
      }
      startSwap(
        candidate,
        { action: null, confirmationToken: null },
        currentSwapOwner(candidate.artifact_id),
      );
    }
  }

  function handlePreparedSwap(pending: PendingD3d12Swap, prepared: PreparedD3d12Swap): void {
    startSwap(pending.candidate, prepared, pending.owner);
  }

  function startSwap(
    candidate: GameCandidate,
    prepared: PreparedD3d12Swap,
    owner: SwapOwner,
  ): void {
    if (!isCurrentSwapOwner(owner)) {
      return;
    }
    pendingArtifactId = candidate.artifact_id;
    const action = prepared.action;
    const presentation: PreparedSwapPresentation | undefined = isD3d12ExecutableMutationAction(
      action,
    )
      ? { action, owner }
      : undefined;
    void onSwap(
      {
        componentId: owner.componentId,
        artifactId: candidate.artifact_id,
        isDownloaded: candidate.is_downloaded,
        confirmationToken: prepared.confirmationToken,
      },
      presentation,
    );
  }

  function handleRollback() {
    if (busy || preflight.planning) {
      return;
    }
    onRollback(component.id);
  }

  function invalidatePendingSwap(): void {
    preflight.cancel();
  }

  function currentSwapOwner(artifactId: string): SwapOwner {
    const candidate = candidates.find((item) => item.artifact_id === artifactId);
    return {
      gameId: component.game_id,
      installPath,
      componentId: component.id,
      artifactId,
      planFingerprint: d3d12PlanFingerprint(
        component.d3d12_executable_status,
        candidate?.d3d12_executable_action ?? null,
      ),
    };
  }

  function isCurrentSwapOwner(owner: SwapOwner): boolean {
    const currentOwner = currentSwapOwner(owner.artifactId);
    return (
      component.game_id === owner.gameId &&
      installPath === owner.installPath &&
      component.id === owner.componentId &&
      candidates.some((candidate) => candidate.artifact_id === owner.artifactId) &&
      currentOwner.planFingerprint === owner.planFingerprint
    );
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
      {#if component.d3d12_executable_status}
        <D3d12ExecutableStatusPanel status={component.d3d12_executable_status} />
      {/if}
    </ItemDescription>
  </ItemContent>
  <ItemActions>
    {#if candidates.length === 0}
      <span class="text-xs text-muted-foreground">{t('gameDetails.version.noReplacements')}</span>
    {:else}
      <DownloadProgressBar ids={progressIds} active={busy} />
      {#if preflight.planning}
        <Loader2Icon
          class="size-4 shrink-0 animate-spin text-muted-foreground"
          aria-label={t('games.loading')}
        />
      {/if}
      <Select
        type="single"
        bind:value={selected}
        disabled={busy || preflight.planning}
        onValueChange={handleSwapChange}
      >
        <SelectTrigger size="sm" class="w-60">
          <span class="truncate">{currentLabel}</span>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={currentValue} label={currentLabel}>{currentLabel}</SelectItem>
          {#if candidatePartition.hasExecutableActions}
            <SelectSeparator />
            {#if candidatePartition.compatible.length > 0}
              <SelectGroup>
                <SelectGroupHeading>
                  {t('gameDetails.d3d12.select.compatible')}
                </SelectGroupHeading>
                {#each candidatePartition.compatible as candidate (candidate.artifact_id)}
                  <ComponentVersionOption {candidate} />
                {/each}
              </SelectGroup>
            {/if}
            {#if candidatePartition.changesExecutable.length > 0}
              {#if candidatePartition.compatible.length > 0}
                <SelectSeparator />
              {/if}
              <SelectGroup>
                <SelectGroupHeading>
                  {t('gameDetails.d3d12.select.changesExecutable')}
                </SelectGroupHeading>
                {#each candidatePartition.changesExecutable as candidate (candidate.artifact_id)}
                  <ComponentVersionOption {candidate} />
                {/each}
              </SelectGroup>
            {/if}
            {#if candidatePartition.unavailable.length > 0}
              {#if candidatePartition.compatible.length > 0 || candidatePartition.changesExecutable.length > 0}
                <SelectSeparator />
              {/if}
              <SelectGroup>
                <SelectGroupHeading>
                  {t('gameDetails.d3d12.select.unavailable')}
                </SelectGroupHeading>
                {#each candidatePartition.unavailable as candidate (candidate.artifact_id)}
                  <ComponentVersionOption {candidate} />
                {/each}
              </SelectGroup>
            {/if}
          {:else}
            {#each candidates as candidate (candidate.artifact_id)}
              <ComponentVersionOption {candidate} />
            {/each}
          {/if}
        </SelectContent>
      </Select>
    {/if}
    {#if component.rollback_available}
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <Button
              {...props}
              variant="ghost"
              size="icon-sm"
              disabled={busy ||
                preflight.planning ||
                component.d3d12_executable_status?.status === 'repair_required'}
              onclick={handleRollback}
              aria-label={t('gameDetails.version.restoreOriginal', { fileName })}
            >
              <Undo2Icon class="size-4" aria-hidden="true" />
            </Button>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent>{t('gameDetails.version.restoreOriginal', { fileName })}</TooltipContent>
      </Tooltip>
    {/if}
  </ItemActions>
</Item>

<DeveloperModeRequirementDialog
  open={preflight.developerModeOpen}
  blocker={preflight.developerModeBlocker}
  retrying={preflight.developerModeRetrying}
  stillDisabledAfterRetry={preflight.developerModeStillDisabledAfterRetry}
  onOpenChange={(open: boolean) => {
    if (!open) {
      invalidatePendingSwap();
    }
  }}
  onRetry={() => void preflight.retry()}
/>
