<script lang="ts">
  import CheckIcon from '@lucide/svelte/icons/check';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import {
    Button,
    buttonVariants,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Separator,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { t, type MessageKey } from '@shared/i18n';
  import type { ExecutableCandidate } from '@features/nvapi-settings';
  import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type { ExecutableLockReason } from '../model/game-executable-lock';
  import GameExecutableTriggerContent from './GameExecutableTriggerContent.svelte';

  const LOCK_TOOLTIP_KEYS = {
    d3d12_managed: 'gameDetails.d3d12.executableLocked',
    d3d12_repair_required: 'gameDetails.d3d12.executableRepairLocked',
  } as const satisfies Record<ExecutableLockReason, MessageKey>;

  type Props = {
    gameId: string;
    exe: GameExecutableContext;
    lockReason?: ExecutableLockReason | null;
  };

  const { gameId, exe, lockReason = null }: Props = $props();

  let open = $state(false);
  // Reset discards a manual choice, so it steps through an inline confirm.
  let confirmingReset = $state(false);

  const locked = $derived(lockReason !== null);
  const isOverride = $derived(exe.effectiveExeSource === 'override');

  const triggerLabel = $derived(exe.effectiveExe ?? t('gameDetails.profile.noExe'));
  const triggerAriaLabel = $derived(
    t('gameDetails.executable.triggerAria', { fileName: triggerLabel }),
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

  function selectCandidate(candidate: ExecutableCandidate): void {
    void exe.setOverride(gameId, candidate.absolute_path);
    open = false;
  }

  function resetToAuto(): void {
    void exe.clearOverride(gameId);
    confirmingReset = false;
    open = false;
  }

  function requestReset(): void {
    confirmingReset = true;
  }

  function cancelReset(): void {
    confirmingReset = false;
  }

  // A newly applied lock closes the selector; every close clears inline confirmation.
  $effect(() => {
    if (locked) {
      open = false;
    }
    if (!open) {
      confirmingReset = false;
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
          aria-label={triggerAriaLabel}
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
            aria-label={triggerAriaLabel}
          >
            <GameExecutableTriggerContent label={triggerLabel} {isOverride} locked={false} />
          </PopoverTrigger>
        {/snippet}
      </TooltipTrigger>

      <PopoverContent align="end" class="w-80 p-0">
        <div class="grid gap-1 p-3">
          <p class="text-sm font-medium">{t('gameDetails.executable.title')}</p>
          <p class="text-xs text-muted-foreground">{t('gameDetails.executable.description')}</p>
          <div class="mt-1 flex items-center justify-between gap-2">
            <span class="text-xs text-muted-foreground">{sourceLabel}</span>
            {#if isOverride}
              {#if confirmingReset}
                <div class="flex items-center gap-1">
                  <Button variant="ghost" size="sm" onclick={cancelReset}>
                    {t('gameDetails.renodx.cancel')}
                  </Button>
                  <Button variant="secondary" size="sm" onclick={resetToAuto}>
                    {t('gameDetails.executable.reset')}
                  </Button>
                </div>
              {:else}
                <Button variant="ghost" size="sm" onclick={requestReset}>
                  <RotateCcwIcon class="size-3.5" aria-hidden="true" />
                  {t('gameDetails.executable.reset')}
                </Button>
              {/if}
            {/if}
          </div>
          {#if confirmingReset}
            <p class="text-xs text-muted-foreground" aria-live="polite">
              {t('gameDetails.executable.resetConfirm')}
            </p>
          {/if}
        </div>

        <Separator />

        <div class="max-h-72 overflow-y-auto p-1">
          {#each candidateGroups as group (group.key)}
            <p class="px-2 py-1 text-xs font-medium text-muted-foreground">
              {group.label}
            </p>
            {#each group.candidates as candidate (candidate.absolute_path)}
              <button
                type="button"
                class="flex w-full items-start gap-2 rounded-sm px-2 py-1.5 text-left hover:bg-accent"
                onclick={() => {
                  selectCandidate(candidate);
                }}
              >
                <CheckIcon
                  class="mt-0.5 size-4 shrink-0 {candidate.file_name === exe.effectiveExe
                    ? 'opacity-100'
                    : 'opacity-0'}"
                  aria-hidden="true"
                />
                <span class="flex min-w-0 flex-col">
                  <span class="truncate text-sm">{candidate.file_name}</span>
                  <span class="truncate text-xs text-muted-foreground">
                    {candidate.relative_path}
                  </span>
                </span>
              </button>
            {/each}
          {/each}

          {#if candidateGroups.length === 0}
            <p class="p-2 text-xs text-muted-foreground">
              {t('gameDetails.profile.noExeDetected')}
            </p>
          {/if}
        </div>
      </PopoverContent>
    </Popover>
  {/if}

  <TooltipContent
    role="tooltip"
    side="bottom"
    align="end"
    sideOffset={6}
    class="max-w-80 whitespace-normal"
  >
    {#if locked}
      <span class="block font-medium">{t('gameDetails.d3d12.executableLockedTitle')}</span>
      <span class="mt-1 block">{tooltipText}</span>
    {:else}
      {tooltipText}
    {/if}
  </TooltipContent>
</Tooltip>
