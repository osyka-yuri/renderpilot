<script lang="ts">
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import {
    Button,
    buttonVariants,
    Popover,
    PopoverContent,
    PopoverTrigger,
    RadioGroup,
    RadioGroupItem,
    Separator,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type { ExecutableLockReason } from '../model/game-executable-lock';
  import GameExecutableTriggerContent from './GameExecutableTriggerContent.svelte';

  const LOCK_TOOLTIP_KEYS = {
    d3d12_managed: 'gameDetails.d3d12.executableLocked',
    d3d12_repair_required: 'gameDetails.d3d12.executableRepairLocked',
  } as const satisfies Record<ExecutableLockReason, MessageKeyWithoutParams>;

  type Props = {
    gameId: string;
    exe: GameExecutableContext;
    lockReason?: ExecutableLockReason | null;
  };

  const { gameId, exe, lockReason = null }: Props = $props();
  const componentId = $props.id();
  const dialogTitleId = `${componentId}-title`;

  let open = $state(false);

  const locked = $derived(lockReason !== null);
  const isOverride = $derived(exe.effectiveExeSource === 'override');

  const triggerLabel = $derived(exe.effectiveExe ?? t('gameDetails.profile.noExe'));
  const executableLabel = $derived(
    t('gameDetails.executable.triggerLabel', { fileName: triggerLabel }),
  );
  const tooltipText = $derived(
    lockReason
      ? t(LOCK_TOOLTIP_KEYS[lockReason])
      : isOverride
        ? t('gameDetails.executable.tooltipCustom')
        : t('gameDetails.executable.tooltipAuto'),
  );
  const sourceLabel = $derived(
    !exe.effectiveExe
      ? t('gameDetails.profile.noExeDetected')
      : isOverride
        ? t('gameDetails.profile.pinnedManual')
        : t('gameDetails.profile.autoDetected'),
  );
  const candidateGroups = $derived(
    [
      {
        key: 'detected',
        label: t('gameDetails.executable.detectedGroup'),
        candidates: exe.supportedCandidates,
      },
      {
        key: 'other',
        label: t('gameDetails.executable.otherGroup'),
        candidates: exe.filteredOutCandidates,
      },
    ].filter((group) => group.candidates.length > 0),
  );

  function selectCandidate(absolutePath: string): void {
    void exe.setOverride(gameId, absolutePath);
    open = false;
  }

  function resetToAuto(): void {
    void exe.clearOverride(gameId);
    open = false;
  }

  // A newly applied lock closes the selector.
  $effect(() => {
    if (locked) {
      open = false;
    }
  });
</script>

<Tooltip>
  {#if locked}
    <TooltipTrigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="sm"
          class="cursor-not-allowed aria-disabled:pointer-events-auto"
          aria-label={executableLabel}
          aria-disabled="true"
        >
          <GameExecutableTriggerContent label={triggerLabel} {isOverride} locked />
        </Button>
      {/snippet}
    </TooltipTrigger>
  {:else}
    <Popover bind:open>
      <TooltipTrigger>
        {#snippet child({ props })}
          <PopoverTrigger
            {...props}
            class={buttonVariants({ variant: 'ghost', size: 'sm' })}
            aria-label={executableLabel}
          >
            <GameExecutableTriggerContent label={triggerLabel} {isOverride} locked={false} />
          </PopoverTrigger>
        {/snippet}
      </TooltipTrigger>

      <PopoverContent role="dialog" aria-labelledby={dialogTitleId} align="end" class="w-80 p-0">
        <div class="grid gap-1 p-3">
          <p id={dialogTitleId} class="text-sm font-medium">
            {t('gameDetails.executable.title')}
          </p>
          <p class="text-xs text-muted-foreground">{t('gameDetails.executable.description')}</p>
          <div class="mt-1 grid gap-2">
            <span class="text-xs text-muted-foreground">{sourceLabel}</span>
            {#if isOverride}
              <Button variant="ghost" size="sm" class="w-fit justify-start" onclick={resetToAuto}>
                <RotateCcwIcon class="size-3.5" aria-hidden="true" />
                {t('gameDetails.executable.reset')}
              </Button>
            {/if}
          </div>
        </div>

        <Separator />

        <RadioGroup
          value={exe.effectiveAbsolutePath ?? ''}
          aria-label={t('gameDetails.executable.groupLabel')}
          class="max-h-72 gap-0 overflow-y-auto p-1"
          onValueChange={selectCandidate}
        >
          {#each candidateGroups as group (group.key)}
            {@const groupLabelId = `${componentId}-${group.key}-label`}
            <div role="group" aria-labelledby={groupLabelId}>
              <p id={groupLabelId} class="px-2 py-1 text-xs font-medium text-muted-foreground">
                {group.label}
              </p>
              {#each group.candidates as candidate, index (candidate.absolute_path)}
                {@const candidateId = `${componentId}-${group.key}-${index}`}
                <label
                  for={candidateId}
                  class="flex min-h-10 w-full cursor-pointer items-start gap-2 rounded-sm px-2 py-1.5 text-start hover:bg-accent has-focus-visible:outline-2 has-focus-visible:-outline-offset-2 has-focus-visible:outline-ring"
                >
                  <RadioGroupItem id={candidateId} value={candidate.absolute_path} class="mt-0.5" />
                  <span class="flex min-w-0 flex-col">
                    <span class="truncate text-sm">{candidate.file_name}</span>
                    <span class="truncate text-xs text-muted-foreground">
                      {candidate.relative_path}
                    </span>
                  </span>
                </label>
              {/each}
            </div>
          {/each}

          {#if candidateGroups.length === 0}
            <p class="p-2 text-xs text-muted-foreground">
              {t('gameDetails.profile.noExeDetected')}
            </p>
          {/if}
        </RadioGroup>
      </PopoverContent>
    </Popover>
  {/if}

  <TooltipContent side="bottom" align="end" sideOffset={6} class="max-w-80 whitespace-normal">
    {#if locked}
      <span class="block font-medium">{t('gameDetails.d3d12.executableLockedTitle')}</span>
      <span class="mt-1 block">{tooltipText}</span>
    {:else}
      {tooltipText}
    {/if}
  </TooltipContent>
</Tooltip>
