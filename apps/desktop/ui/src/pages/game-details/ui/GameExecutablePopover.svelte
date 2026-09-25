<script lang="ts">
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import {
    Button,
    buttonVariants,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
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
  import { formatPresentedError } from '@shared/error-presentation';
  import type { GameExecutableContext } from '../model/create-game-executable-context.svelte';
  import type {
    ExecutableLockReason,
    ProfileSelectionBlockReason,
  } from '../model/game-executable-lock';
  import GameExecutableTriggerContent from './GameExecutableTriggerContent.svelte';

  const LOCK_TOOLTIP_KEYS = {
    d3d12_managed: 'gameDetails.d3d12.executableLocked',
    d3d12_repair_required: 'gameDetails.d3d12.executableRepairLocked',
  } as const satisfies Record<ExecutableLockReason, MessageKeyWithoutParams>;

  type Props = {
    gameId: string;
    exe: GameExecutableContext;
    lockReason?: ExecutableLockReason | null;
    ownedBindingPath?: string | null;
    profileSelectionBlockReason?: ProfileSelectionBlockReason | null;
    onMoveProfile?: (path: string, selectAutomatically: boolean) => boolean | Promise<boolean>;
  };

  type CandidateTooltipTriggerProps = {
    onfocus?: (event: FocusEvent) => void;
    onblur?: (event: FocusEvent) => void;
    'aria-describedby'?: string;
    [key: string]: unknown;
  };

  const {
    gameId,
    exe,
    lockReason = null,
    ownedBindingPath = null,
    profileSelectionBlockReason = null,
    onMoveProfile,
  }: Props = $props();
  const componentId = $props.id();
  const dialogTitleId = `${componentId}-title`;

  let open = $state(false);
  let pendingMovePath = $state<string | null>(null);
  let pendingMoveGameId = $state<string | null>(null);
  let pendingMoveSourcePath = $state<string | null>(null);
  let moveToAutomatic = $state(false);
  let movingProfile = $state(false);
  let moveError = $state<string | null>(null);

  const locked = $derived(lockReason !== null || profileSelectionBlockReason !== null);
  const isOverride = $derived(exe.effectiveExeSource === 'override');

  const triggerLabel = $derived(exe.effectiveExe ?? t('gameDetails.profile.noExe'));
  const executableLabel = $derived(
    t('gameDetails.executable.triggerLabel', { fileName: triggerLabel }),
  );
  const tooltipText = $derived(
    lockReason
      ? t(LOCK_TOOLTIP_KEYS[lockReason])
      : profileSelectionBlockReason === 'checking'
        ? t('gameDetails.profile.executableSelectionChecking')
        : profileSelectionBlockReason === 'unverified'
          ? t('gameDetails.profile.executableSelectionBlocked')
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

  async function selectCandidate(absolutePath: string): Promise<void> {
    if (locked) {
      return;
    }
    if (
      ownedBindingPath &&
      ownedBindingPath.toLocaleLowerCase() !== absolutePath.toLocaleLowerCase()
    ) {
      pendingMovePath = absolutePath;
      pendingMoveGameId = gameId;
      pendingMoveSourcePath = ownedBindingPath;
      moveToAutomatic = false;
      moveError = null;
      open = false;
      return;
    }
    open = !(await exe.setOverride(gameId, absolutePath));
  }

  async function resetToAuto(): Promise<void> {
    if (locked) {
      return;
    }
    const automaticPath = exe.autoAbsolutePath;
    if (
      ownedBindingPath &&
      automaticPath &&
      ownedBindingPath.toLocaleLowerCase() !== automaticPath.toLocaleLowerCase()
    ) {
      pendingMovePath = automaticPath;
      pendingMoveGameId = gameId;
      pendingMoveSourcePath = ownedBindingPath;
      moveToAutomatic = true;
      moveError = null;
      open = false;
      return;
    }
    open = !(await exe.clearOverride(gameId));
  }

  async function confirmProfileMove(): Promise<void> {
    const path = pendingMovePath;
    if (
      !path ||
      locked ||
      movingProfile ||
      !onMoveProfile ||
      pendingMoveGameId !== gameId ||
      pendingMoveSourcePath !== ownedBindingPath
    ) {
      return;
    }
    movingProfile = true;
    try {
      const succeeded = await onMoveProfile(path, moveToAutomatic);
      if (succeeded) {
        clearPendingMove();
      } else {
        moveError = t('gameDetails.profile.moveFailed');
      }
    } catch (error) {
      moveError = formatPresentedError(error);
    } finally {
      movingProfile = false;
    }
  }

  function requestMoveDialogOpen(next: boolean): void {
    if (!movingProfile && !next) {
      clearPendingMove();
    }
  }

  function clearPendingMove(): void {
    pendingMovePath = null;
    pendingMoveGameId = null;
    pendingMoveSourcePath = null;
    moveError = null;
  }

  // A newly applied lock closes the selector.
  $effect(() => {
    if (locked) {
      open = false;
    }
    if (
      pendingMovePath &&
      (pendingMoveGameId !== gameId ||
        (!movingProfile && (locked || pendingMoveSourcePath !== ownedBindingPath)))
    ) {
      clearPendingMove();
    }
  });
</script>

<Tooltip disabled={open}>
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
          {#if exe.changeError}
            <p role="alert" class="text-xs text-destructive">
              <span class="font-medium">{t('gameDetails.executable.changeFailed')}</span>
              <span class="block wrap-break-word">{exe.changeError}</span>
            </p>
          {:else if exe.refreshError}
            <p role="status" class="text-xs text-warning">
              <span class="font-medium">{t('gameDetails.executable.refreshFailed')}</span>
              <span class="block wrap-break-word">{exe.refreshError}</span>
            </p>
          {:else if exe.loadError}
            <p role="alert" class="text-xs text-destructive">
              <span class="font-medium">{t('gameDetails.executable.loadFailed')}</span>
              <span class="block wrap-break-word">{exe.loadError}</span>
            </p>
          {/if}
        </div>

        <Separator />

        <RadioGroup
          value={exe.effectiveAbsolutePath ?? ''}
          aria-label={t('gameDetails.executable.groupLabel')}
          class="max-h-72 min-w-0 grid-cols-1 gap-0 overflow-x-hidden overflow-y-auto p-1"
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
                <Tooltip ignoreNonKeyboardFocus>
                  <TooltipTrigger>
                    {#snippet child({ props }: { props: CandidateTooltipTriggerProps })}
                      {@const {
                        onfocus,
                        onblur,
                        tabindex: _tabindex,
                        'aria-describedby': ariaDescribedby,
                        ...labelProps
                      } = props}
                      <label
                        {...labelProps}
                        for={candidateId}
                        class="flex min-h-10 w-full cursor-pointer items-start gap-2 rounded-sm px-2 py-1.5 text-start hover:bg-accent has-focus-visible:outline-2 has-focus-visible:-outline-offset-2 has-focus-visible:outline-ring"
                      >
                        <RadioGroupItem
                          id={candidateId}
                          value={candidate.absolute_path}
                          class="mt-0.5 shrink-0"
                          {onfocus}
                          {onblur}
                          aria-describedby={ariaDescribedby}
                        />
                        <span class="flex min-w-0 flex-1 flex-col overflow-hidden">
                          <span class="truncate text-sm">{candidate.file_name}</span>
                          <span class="truncate text-xs text-muted-foreground">
                            {candidate.relative_path}
                          </span>
                        </span>
                      </label>
                    {/snippet}
                  </TooltipTrigger>
                  <TooltipContent side="left" sideOffset={6} class="max-w-xs wrap-break-word">
                    <span class="block font-medium">{candidate.file_name}</span>
                    {#if candidate.relative_path && candidate.relative_path !== candidate.file_name}
                      <span class="mt-0.5 block opacity-80">{candidate.relative_path}</span>
                    {/if}
                  </TooltipContent>
                </Tooltip>
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
    {#if lockReason}
      <span class="block font-medium">{t('gameDetails.d3d12.executableLockedTitle')}</span>
      <span class="mt-1 block">{tooltipText}</span>
    {:else}
      {tooltipText}
    {/if}
  </TooltipContent>
</Tooltip>

<Dialog open={pendingMovePath !== null} onOpenChange={requestMoveDialogOpen}>
  <DialogContent closeLabel={t('common.close')}>
    <DialogHeader>
      <DialogTitle>{t('gameDetails.profile.moveConfirmTitle')}</DialogTitle>
      <DialogDescription>
        {t('gameDetails.profile.moveConfirmDescription', {
          from: pendingMoveSourcePath ?? '',
          to: pendingMovePath ?? '',
        })}
      </DialogDescription>
      {#if moveError}
        <p role="alert" class="text-sm text-destructive">{moveError}</p>
      {/if}
    </DialogHeader>
    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        disabled={movingProfile}
        onclick={() => {
          requestMoveDialogOpen(false);
        }}
      >
        {t('common.cancel')}
      </Button>
      <Button size="sm" disabled={movingProfile} onclick={() => void confirmProfileMove()}>
        {#if movingProfile}<Loader2Icon class="animate-spin" aria-hidden="true" />{/if}
        {t('gameDetails.profile.moveConfirmAction')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
