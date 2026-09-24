<script lang="ts">
  import { tick } from 'svelte';
  import Loader2Icon from '@lucide/svelte/icons/loader-2';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import {
    Alert,
    AlertDescription,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Item,
    ItemActions,
    ItemContent,
  } from '@shared/ui';
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';
  import type { NvapiProfileStatus } from '@features/nvapi-settings';
  import type { NvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';

  type Props = {
    gameId: string;
    mode: 'settings' | 'recovery';
    profile: NvapiProfileContext;
    onOpenGameDetails: (gameId: string) => void | Promise<void>;
    onRecoveryDeleteComplete: () => void;
    canCreate: boolean;
  };

  type DeleteTarget = Pick<NvapiProfileStatus, 'selectedExecutable' | 'bindingPath'> & {
    profileName: string;
    gameId: string;
  };

  type ActiveAction = {
    kind: 'create' | 'delete' | 'retry';
    gameId: string;
  };

  const { gameId, mode, profile, onOpenGameDetails, onRecoveryDeleteComplete, canCreate }: Props =
    $props();
  const componentId = $props.id();
  const blockedDescriptionId = `${componentId}-delete-blocked`;
  let activeAction = $state.raw<ActiveAction | null>(null);
  let deleteTarget = $state<DeleteTarget | null>(null);
  let deleteConfirmOpen = $state(false);
  let focusControlGameId = $state<string | null>(null);
  let controlRow = $state<HTMLDivElement | null>(null);

  const statusKeys = {
    noExecutable: 'gameDetails.profile.state.noExecutable',
    nvapiUnavailable: 'gameDetails.profile.state.nvapiUnavailable',
    ambiguous: 'gameDetails.profile.state.ambiguous',
    error: 'gameDetails.profile.state.error',
    missing: 'gameDetails.profile.state.missing',
    predefined: undefined,
    external: undefined,
    owned: undefined,
    ownedByAnotherGame: 'gameDetails.profile.state.ownedByAnotherGame',
    conflict: 'gameDetails.profile.state.conflict',
    pending: 'gameDetails.profile.state.pending',
  } satisfies Record<NvapiProfileStatus['state'], MessageKeyWithoutParams | undefined>;

  const warningStates = new Set<NvapiProfileStatus['state']>([
    'nvapiUnavailable',
    'ambiguous',
    'error',
    'ownedByAnotherGame',
    'conflict',
    'pending',
  ]);
  const status = $derived(profile.status);
  const currentAction = $derived(activeAction?.gameId === gameId ? activeAction : null);
  const isOwnedByThisGame = $derived(status?.ownedByThisGame === true);
  const isSameGamePending = $derived(
    status?.state === 'pending' &&
      !!status.pendingOperation &&
      status.pendingOperationGameId === gameId,
  );
  const hasUnmanagedProfile = $derived(
    status?.state === 'predefined' || status?.state === 'external',
  );
  const showRecoveryOnly = $derived(
    mode === 'recovery' &&
      ((!hasUnmanagedProfile && (isOwnedByThisGame || isSameGamePending)) ||
        profile.loadError !== null ||
        status?.state === 'error' ||
        (currentAction?.kind === 'retry' && profile.loading)),
  );
  const canCreateProfile = $derived(
    mode === 'settings' && canCreate && status?.state === 'missing' && status.canCreate,
  );
  const showControl = $derived(mode === 'settings' || showRecoveryOnly);
  const isChecking = $derived(
    profile.loading ||
      currentAction?.kind === 'retry' ||
      (mode === 'settings' && status === null && profile.loadError === null),
  );
  const isRowBusy = $derived(isChecking || currentAction !== null);
  const statusText = $derived.by(() => {
    if (isChecking) {
      return t('gameDetails.profile.checking');
    }
    if (
      status?.state === 'owned' ||
      status?.state === 'predefined' ||
      status?.state === 'external'
    ) {
      return status.profileName ?? t('gameDetails.profile.state.available');
    }
    if (status) {
      return t(statusKeys[status.state]);
    }
    if (profile.loadError !== null) {
      return t('gameDetails.profile.state.error');
    }
    if (profile.actionError !== null || profile.refreshError !== null) {
      return t('gameDetails.profile.state.available');
    }
    return t('gameDetails.profile.checking');
  });
  const showNamedProfile = $derived(
    !isChecking &&
      (status?.state === 'owned' || status?.state === 'predefined' || status?.state === 'external'),
  );
  const isWarning = $derived(status ? warningStates.has(status.state) : profile.loadError !== null);
  const statusNeedsRetry = $derived(
    profile.loadError !== null || status?.state === 'error' || status?.state === 'nvapiUnavailable',
  );
  const showDeleteAction = $derived(
    isOwnedByThisGame &&
      !hasUnmanagedProfile &&
      status?.state !== 'pending' &&
      status?.state !== 'error' &&
      status?.state !== 'nvapiUnavailable',
  );
  const canDeleteCurrentProfile = $derived(
    showDeleteAction &&
      status?.state === 'owned' &&
      status.canDelete &&
      status.profileName !== null &&
      !profile.loading &&
      !profile.busy,
  );
  const deletionBlocked = $derived(
    showDeleteAction &&
      (status?.state !== 'owned' || !status.canDelete || status.profileName === null),
  );
  const deleteTargetMatches = $derived(isCurrentDeleteTarget(deleteTarget));
  const isBusy = $derived(
    profile.busy ||
      profile.loading ||
      (mode === 'settings' && status === null && profile.loadError === null),
  );
  const showProfileAction = $derived(!isChecking || currentAction !== null);

  $effect(() => {
    if (deleteConfirmOpen && !deleteTargetMatches) {
      closeDeleteDialog();
    }
  });

  function isCurrentDeleteTarget(target: DeleteTarget | null): target is DeleteTarget {
    return (
      !!target &&
      canDeleteCurrentProfile &&
      gameId === target.gameId &&
      status?.selectedExecutable === target.selectedExecutable &&
      status.bindingPath === target.bindingPath &&
      status.profileName === target.profileName
    );
  }

  function requestDeleteDialogOpen(open: boolean): void {
    if (!open) {
      closeDeleteDialog();
      return;
    }
    if (canDeleteCurrentProfile && status?.state === 'owned' && status.profileName !== null) {
      deleteTarget = {
        gameId,
        selectedExecutable: status.selectedExecutable,
        bindingPath: status.bindingPath,
        profileName: status.profileName,
      };
      deleteConfirmOpen = true;
    }
  }

  function closeDeleteDialog(): void {
    deleteConfirmOpen = false;
  }

  function beginAction(kind: ActiveAction['kind'], actionGameId: string): ActiveAction {
    const action = { kind, gameId: actionGameId };
    activeAction = action;
    return action;
  }

  function finishAction(action: ActiveAction): void {
    if (activeAction === action) {
      activeAction = null;
    }
  }

  async function createProfile(): Promise<void> {
    if (!canCreateProfile || isBusy) {
      return;
    }
    const action = beginAction('create', gameId);
    try {
      await profile.create(action.gameId);
    } finally {
      finishAction(action);
    }
  }

  async function confirmDelete(): Promise<void> {
    const target = deleteTarget;
    if (!isCurrentDeleteTarget(target)) {
      closeDeleteDialog();
      return;
    }
    closeDeleteDialog();
    focusControlGameId = target.gameId;
    const action = beginAction('delete', target.gameId);
    let removed = false;
    try {
      removed = await profile.remove(target.gameId);
    } finally {
      finishAction(action);
    }
    if (!removed || mode !== 'recovery' || gameId !== target.gameId) {
      return;
    }

    await tick();
    if (gameId !== target.gameId) {
      return;
    }
    if (!showControl) {
      onRecoveryDeleteComplete();
    } else {
      controlRow?.focus({ preventScroll: true });
    }
  }

  async function retry(): Promise<void> {
    if (profile.busy || profile.loading) {
      return;
    }
    const action = beginAction('retry', gameId);
    try {
      await profile.reload(action.gameId);
    } finally {
      finishAction(action);
    }
  }

  function openPendingGameDetails(pendingGameId: string | null): void {
    if (pendingGameId) {
      void onOpenGameDetails(pendingGameId);
    }
  }
</script>

{#if showControl}
  <div class="grid w-full min-w-0 gap-2">
    <Item
      bind:ref={controlRow}
      variant="outline"
      size="sm"
      class="h-16 min-w-0 flex-nowrap bg-card py-1"
      role="group"
      aria-label={t('gameDetails.profile.title')}
      aria-busy={isRowBusy}
      tabindex={-1}
    >
      {#if isRowBusy}
        <Loader2Icon
          class="size-4 shrink-0 animate-spin text-muted-foreground"
          aria-hidden="true"
        />
      {:else if isWarning}
        <TriangleAlertIcon class="size-4 shrink-0 text-warning" aria-hidden="true" />
      {/if}

      <ItemContent class="min-w-0">
        {#if showNamedProfile}
          <div role="status">
            <p class="text-xs text-muted-foreground">{t('gameDetails.profile.title')}</p>
            <p class="line-clamp-2 text-sm font-medium wrap-break-word">{statusText}</p>
          </div>
        {:else}
          <p
            class="truncate text-sm"
            class:text-muted-foreground={!isWarning}
            class:text-warning={isWarning}
            role={profile.loadError !== null ? undefined : isWarning ? 'alert' : 'status'}
          >
            {statusText}
          </p>
        {/if}
      </ItemContent>

      <ItemActions class="shrink-0">
        {#if showProfileAction && canCreateProfile}
          <Button
            size="sm"
            disabled={profile.busy || profile.loading}
            aria-busy={currentAction?.kind === 'create' || currentAction?.kind === 'delete'}
            onclick={() => void createProfile()}
          >
            {#if currentAction?.kind === 'create' || currentAction?.kind === 'delete'}
              <Loader2Icon class="animate-spin" aria-hidden="true" />
            {/if}
            {#if currentAction?.kind === 'create'}
              {t('gameDetails.profile.creating')}
            {:else if currentAction?.kind === 'delete'}
              {t('gameDetails.profile.deleting')}
            {:else}
              {t('gameDetails.profile.create')}
            {/if}
          </Button>
        {:else if showProfileAction && statusNeedsRetry}
          <Button
            variant="secondary"
            size="sm"
            disabled={isBusy}
            aria-busy={currentAction?.kind === 'retry'}
            onclick={() => void retry()}
          >
            {#if currentAction?.kind === 'retry'}<Loader2Icon
                class="animate-spin"
                aria-hidden="true"
              />{/if}
            {currentAction?.kind === 'retry'
              ? t('gameDetails.profile.checking')
              : t('gameDetails.profile.retry')}
          </Button>
        {:else if showProfileAction && showDeleteAction}
          <Button
            variant="destructive"
            size="sm"
            disabled={deletionBlocked || isBusy}
            aria-describedby={deletionBlocked ? blockedDescriptionId : undefined}
            aria-busy={currentAction?.kind === 'delete'}
            onclick={() => {
              requestDeleteDialogOpen(true);
            }}
          >
            {#if currentAction?.kind === 'delete' || currentAction?.kind === 'create'}
              <Loader2Icon class="animate-spin" aria-hidden="true" />
            {:else}
              <Trash2Icon aria-hidden="true" />
            {/if}
            {#if currentAction?.kind === 'delete'}
              {t('gameDetails.profile.deleting')}
            {:else if currentAction?.kind === 'create'}
              {t('gameDetails.profile.creating')}
            {:else}
              {t('gameDetails.profile.delete')}
            {/if}
          </Button>
        {:else if showProfileAction && isSameGamePending}
          <Button
            variant="secondary"
            size="sm"
            disabled={isBusy}
            aria-busy={currentAction?.kind === 'retry'}
            onclick={() => void retry()}
          >
            {#if currentAction?.kind === 'retry'}<Loader2Icon
                class="animate-spin"
                aria-hidden="true"
              />{/if}
            {currentAction?.kind === 'retry'
              ? t('gameDetails.profile.checking')
              : t('gameDetails.profile.retryRecovery')}
          </Button>
        {/if}
      </ItemActions>

      {#if currentAction?.kind === 'create' || currentAction?.kind === 'delete'}
        <span class="sr-only" role="status" aria-live="polite">
          {currentAction.kind === 'create'
            ? t('gameDetails.profile.creating')
            : t('gameDetails.profile.deleting')}
        </span>
      {/if}
    </Item>

    {#if deletionBlocked}
      <p id={blockedDescriptionId} class="px-1 text-xs text-muted-foreground">
        {t('gameDetails.profile.deleteBlocked')}
      </p>
    {/if}

    {#if status?.state === 'conflict' && status.bindingPath && status.bindingPath !== status.selectedExecutable}
      <p class="px-1 text-xs text-muted-foreground">
        <span>{t('gameDetails.profile.boundExecutable')}: </span>
        <code class="break-all">{status.bindingPath}</code>
      </p>
    {/if}

    {#if status?.state === 'pending' && status.pendingOperation && !isSameGamePending}
      <div class="flex flex-wrap items-center gap-2 px-1">
        <p class="min-w-0 flex-1 text-sm text-muted-foreground" role="status">
          {#if status.pendingOperationGameId}
            {t('gameDetails.profile.pendingOtherGame')}
          {:else}
            {t('gameDetails.profile.pendingGlobal')}
          {/if}
        </p>
        {#if status.pendingOperationGameId}
          <Button
            variant="secondary"
            size="sm"
            disabled={isBusy}
            onclick={() => {
              openPendingGameDetails(status.pendingOperationGameId);
            }}
          >
            {t('gameDetails.profile.openGame')}
          </Button>
        {/if}
      </div>
    {/if}

    {#if profile.loadError}
      <Alert variant="warning" size="sm" role="alert" class="mx-1">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{profile.loadError}</AlertDescription>
      </Alert>
    {/if}

    {#if profile.actionError}
      <Alert variant="warning" size="sm" role="alert" class="mx-1">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{profile.actionError}</AlertDescription>
      </Alert>
    {/if}

    {#if profile.refreshError}
      <Alert variant="warning" size="sm" role="status" class="mx-1">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{profile.refreshError}</AlertDescription>
      </Alert>
    {/if}
  </div>
{/if}

<Dialog open={deleteConfirmOpen && deleteTargetMatches} onOpenChange={requestDeleteDialogOpen}>
  <DialogContent
    closeLabel={t('common.close')}
    onCloseAutoFocus={(event: Event) => {
      const targetGameId = focusControlGameId;
      if (targetGameId !== null) {
        event.preventDefault();
        focusControlGameId = null;
        if (targetGameId === gameId) {
          controlRow?.focus({ preventScroll: true });
        }
      }
    }}
  >
    <DialogHeader>
      {#if deleteTarget}
        <DialogTitle class="pe-10 wrap-break-word">
          {t('gameDetails.profile.deleteConfirmTitle', { profileName: deleteTarget.profileName })}
        </DialogTitle>
      {/if}
      <DialogDescription>{t('gameDetails.profile.deleteConfirmDescription')}</DialogDescription>
    </DialogHeader>
    <DialogFooter>
      <Button
        variant="secondary"
        size="sm"
        disabled={profile.busy}
        onclick={() => {
          requestDeleteDialogOpen(false);
        }}
      >
        {t('common.cancel')}
      </Button>
      <Button
        variant="destructive"
        size="sm"
        disabled={profile.busy || !deleteTargetMatches}
        onclick={() => void confirmDelete()}
      >
        {t('gameDetails.profile.deleteConfirmAction')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
