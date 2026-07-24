<script lang="ts">
  import AppWindowIcon from '@lucide/svelte/icons/app-window';
  import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
  import CheckIcon from '@lucide/svelte/icons/check';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import {
    Badge,
    Button,
    buttonVariants,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Separator,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { ExecutableCandidate } from '@features/nvapi-settings';
  import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';

  type Props = {
    gameId: string;
    exe: GameExecutableContext;
    locked?: boolean;
  };

  const { gameId, exe, locked = false }: Props = $props();

  let open = $state(false);
  // Reset discards a manual choice, so it steps through an inline confirm.
  let confirmingReset = $state(false);

  const isOverride = $derived(exe.effectiveExeSource === 'override');

  const triggerLabel = $derived(exe.effectiveExe ?? t('gameDetails.profile.noExe'));
  const triggerTitle = $derived(
    locked
      ? t('gameDetails.d3d12.executableLocked')
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

  function selectCandidate(candidate: ExecutableCandidate): void {
    void exe.setOverride(gameId, candidate.absolute_path);
    open = false;
  }

  function resetToAuto(): void {
    void exe.clearOverride(gameId);
    confirmingReset = false;
    open = false;
  }

  // Reset the inline-confirm state whenever the popover closes.
  $effect(() => {
    if (!open) {
      confirmingReset = false;
    }
  });
</script>

<Popover bind:open>
  <PopoverTrigger
    class={buttonVariants({ variant: 'ghost', size: 'sm' })}
    title={triggerTitle}
    aria-label={triggerTitle}
    disabled={locked}
  >
    <AppWindowIcon class="size-4 opacity-70" aria-hidden="true" />
    <span class="max-w-40 truncate">{triggerLabel}</span>
    {#if isOverride}
      <Badge variant="secondary" class="px-1.5 py-0 text-[10px]">
        {t('gameDetails.executable.customBadge')}
      </Badge>
    {/if}
    <ChevronDownIcon class="size-3.5 opacity-50" aria-hidden="true" />
  </PopoverTrigger>

  <PopoverContent align="end" class="w-80 p-0">
    <div class="grid gap-1 p-3">
      <p class="text-sm font-medium">{t('gameDetails.executable.title')}</p>
      <p class="text-xs text-muted-foreground">{t('gameDetails.executable.description')}</p>
      <div class="mt-1 flex items-center justify-between gap-2">
        <span class="text-xs text-muted-foreground">{sourceLabel}</span>
        {#if isOverride}
          {#if confirmingReset}
            <div class="flex items-center gap-1">
              <Button variant="ghost" size="sm" onclick={() => (confirmingReset = false)}>
                {t('gameDetails.renodx.cancel')}
              </Button>
              <Button variant="secondary" size="sm" onclick={resetToAuto}>
                {t('gameDetails.executable.reset')}
              </Button>
            </div>
          {:else}
            <Button variant="ghost" size="sm" onclick={() => (confirmingReset = true)}>
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
      {#if exe.supportedCandidates.length > 0}
        <p class="px-2 py-1 text-xs font-medium text-muted-foreground">
          {t('gameDetails.executable.detectedGroup')}
        </p>
        {#each exe.supportedCandidates as candidate (candidate.absolute_path)}
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
              <span class="truncate text-xs text-muted-foreground">{candidate.relative_path}</span>
            </span>
          </button>
        {/each}
      {/if}

      {#if exe.filteredOutCandidates.length > 0}
        <p class="px-2 py-1 text-xs font-medium text-muted-foreground">
          {t('gameDetails.executable.otherGroup')}
        </p>
        {#each exe.filteredOutCandidates as candidate (candidate.absolute_path)}
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
              <span class="truncate text-xs text-muted-foreground">{candidate.relative_path}</span>
            </span>
          </button>
        {/each}
      {/if}

      {#if exe.supportedCandidates.length === 0 && exe.filteredOutCandidates.length === 0}
        <p class="p-2 text-xs text-muted-foreground">
          {t('gameDetails.profile.noExeDetected')}
        </p>
      {/if}
    </div>
  </PopoverContent>
</Popover>
