<script lang="ts">
  import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
  import StarIcon from '@lucide/svelte/icons/star';
  import StarOffIcon from '@lucide/svelte/icons/star-off';
  import EyeIcon from '@lucide/svelte/icons/eye';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import type { Component } from 'svelte';

  import { t } from '@shared/i18n';
  import { publishPresentedErrorNotification } from '@shared/notifications';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    Button,
    AlertDialog,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
    ScrollArea,
    Spinner,
    buttonVariants,
  } from '@shared/ui';
  import { assessGameRemoval } from '../api/desktop';
  import { createRemoveGameFlow } from '../model/remove-game-flow.svelte';
  import type { NvapiClaimAbandonmentAuthorization } from '../model/types';

  type MenuActionId =
    | 'fetch-cover'
    | 'pick-cover'
    | 'clear-cover'
    | 'toggle-favorite'
    | 'toggle-hidden'
    | 'remove-from-catalog';

  type MenuActionHandler = () => void | Promise<void>;
  type ConfirmActionHandler = (
    authorization?: NvapiClaimAbandonmentAuthorization,
  ) => boolean | Promise<boolean>;

  type MenuAction = {
    readonly id: MenuActionId;
    readonly label: string;
    readonly title: string;
    readonly disabled: boolean;
    readonly danger?: boolean;
    readonly icon: Component;
    readonly handler: MenuActionHandler;
  };

  type Props = {
    gameId: string;
    title: string;
    disabled?: boolean;
    pickDisabled?: boolean;
    autoFetchInProgress?: boolean;
    hasCover?: boolean;
    isFavorite?: boolean;
    isHidden?: boolean;
    canRemoveFromCatalog?: boolean;
    open?: boolean;

    onOpenChange?: (next: boolean) => void;
    onFetchCover?: MenuActionHandler;
    onPickCover?: MenuActionHandler;
    onClearCover?: MenuActionHandler;
    onToggleFavorite?: MenuActionHandler;
    onToggleHidden?: MenuActionHandler;
    onRemoveFromCatalog?: ConfirmActionHandler;
  };

  const noop: MenuActionHandler = () => undefined;
  const noopOpenChange = () => undefined;

  let {
    gameId,
    title,
    disabled = false,
    pickDisabled = false,
    autoFetchInProgress = false,
    hasCover = false,
    isFavorite = false,
    isHidden = false,
    canRemoveFromCatalog = false,
    open = false,

    onOpenChange = noopOpenChange,
    onFetchCover = noop,
    onPickCover = noop,
    onClearCover = noop,
    onToggleFavorite = noop,
    onToggleHidden = noop,
    onRemoveFromCatalog = () => false,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);

  const removeFlow = createRemoveGameFlow({
    assess: assessGameRemoval,
    remove: async (id, authorization) => {
      if (authorization && authorization.gameId !== id) {
        throw new Error(
          `Authorization identity mismatch: expected ${id}, received ${authorization.gameId}`,
        );
      }
      return await onRemoveFromCatalog(authorization);
    },
  });

  const isMenuOpen = $derived(open && !disabled);
  const gameOptionsLabel = $derived(t('game.card.menu.label', { title }));

  const isFetchCoverDisabled = $derived(disabled || autoFetchInProgress);
  const isPickCoverDisabled = $derived(disabled || pickDisabled);
  const isClearCoverDisabled = $derived(disabled || !hasCover);

  const menuActionGroups = $derived.by((): MenuAction[][] => [
    [
      {
        id: 'toggle-favorite',
        label: isFavorite ? t('game.card.menu.favorite.remove') : t('game.card.menu.favorite.add'),
        title: t('game.card.menu.favorite.toggleHint'),
        disabled: disabled,
        icon: isFavorite ? StarOffIcon : StarIcon,
        handler: onToggleFavorite,
      },
      {
        id: 'toggle-hidden',
        label: isHidden ? t('game.card.menu.hidden.remove') : t('game.card.menu.hidden.add'),
        title: t('game.card.menu.hidden.toggleHint'),
        disabled: disabled,
        icon: isHidden ? EyeIcon : EyeOffIcon,
        handler: onToggleHidden,
      },
    ],
    [
      {
        id: 'fetch-cover',
        label: autoFetchInProgress ? t('game.cover.menu.fetching') : t('game.cover.menu.fetch'),
        disabled: isFetchCoverDisabled,
        title: t('game.cover.menu.fetchHint'),
        icon: DownloadIcon,
        handler: onFetchCover,
      },
      {
        id: 'pick-cover',
        label: t('game.cover.menu.pick'),
        disabled: isPickCoverDisabled,
        title: t('game.cover.menu.pickHint'),
        icon: FolderOpenIcon,
        handler: onPickCover,
      },
      {
        id: 'clear-cover',
        label: t('game.cover.menu.clear'),
        disabled: isClearCoverDisabled,
        danger: true,
        title: t('game.cover.menu.clearHint'),
        icon: Trash2Icon,
        handler: onClearCover,
      },
    ],
    ...(canRemoveFromCatalog
      ? [
          [
            {
              id: 'remove-from-catalog' as const,
              label: t('game.card.menu.removeFromCatalog'),
              disabled: disabled || removeFlow.submitting,
              danger: true,
              title: t('game.card.menu.removeFromCatalogHint'),
              icon: Trash2Icon,
              handler: () => {
                void removeFlow.openConfirmation(gameId, title);
              },
            },
          ],
        ]
      : []),
  ]);

  $effect(() => {
    if (disabled && open) {
      onOpenChange(false);
    }
  });

  export function focusTrigger(): void {
    triggerEl?.focus({ preventScroll: true });
  }

  function handleMenuOpenChange(nextOpen: boolean): void {
    if (nextOpen && disabled) {
      return;
    }

    onOpenChange(nextOpen);
  }

  async function runMenuAction(action: MenuAction): Promise<void> {
    try {
      await action.handler();
    } catch (error) {
      publishPresentedErrorNotification(t('notify.statusError'), error);
    }
  }

  function handleMenuActionClick(action: MenuAction): void {
    if (action.disabled) {
      return;
    }

    onOpenChange(false);
    void runMenuAction(action);
  }
</script>

<DropdownMenu open={isMenuOpen} onOpenChange={handleMenuOpenChange}>
  <DropdownMenuTrigger
    data-game-focus-target="menu"
    bind:ref={triggerEl}
    class={buttonVariants({ variant: 'outline', size: 'icon-sm' })}
    aria-label={gameOptionsLabel}
    {disabled}
  >
    <EllipsisIcon class="block size-4 shrink-0" aria-hidden="true" />
  </DropdownMenuTrigger>

  <DropdownMenuContent
    align="end"
    sideOffset={6}
    loop
    aria-label={gameOptionsLabel}
    class="min-w-56 motion-reduce:animate-none"
  >
    {#each menuActionGroups as group, i (i)}
      {#if i > 0}
        <DropdownMenuSeparator />
      {/if}
      {#each group as action (action.id)}
        <DropdownMenuItem
          class="min-h-8"
          variant={action.danger === true ? 'destructive' : 'default'}
          disabled={action.disabled}
          title={action.title}
          textValue={action.label}
          onSelect={() => {
            handleMenuActionClick(action);
          }}
        >
          {@const Icon = action.icon}
          <Icon class="size-4 shrink-0" aria-hidden="true" />
          <span class="flex-1 truncate">{action.label}</span>
        </DropdownMenuItem>
      {/each}
    {/each}
  </DropdownMenuContent>
</DropdownMenu>

{#if canRemoveFromCatalog}
  <AlertDialog open={removeFlow.open} onOpenChange={removeFlow.setOpen}>
    <AlertDialogContent
      class="sm:max-w-md"
      escapeKeydownBehavior={removeFlow.submitting ? 'ignore' : 'close'}
    >
      <AlertDialogHeader>
        <AlertDialogTitle
          >{t('game.card.removeConfirm.title', { title: removeFlow.gameTitle })}</AlertDialogTitle
        >
        <AlertDialogDescription>
          {t('game.card.removeConfirm.description')}
        </AlertDialogDescription>

        {#if removeFlow.isDiverged}
          <Alert
            variant="warning"
            size="sm"
            role="status"
            class="mt-2 text-start"
            data-diverged-claims-warning
          >
            <TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
            <AlertDescription>
              {t('game.card.removeConfirm.divergedWarning')}
            </AlertDescription>
          </Alert>

          {#if removeFlow.divergedClaims.length > 0}
            <div class="mt-2 flex flex-col gap-1.5 text-start text-xs">
              <span class="font-medium text-muted-foreground">
                {t('game.card.removeConfirm.divergedSettingsTitle')}
              </span>
              <ScrollArea
                class="max-h-32 rounded-md border bg-muted/30 p-2"
                data-diverged-settings-list
              >
                <ul class="flex flex-col gap-1.5">
                  {#each removeFlow.divergedClaims as claim (`${claim.targetId}:${claim.settingId}`)}
                    <li class="flex items-center justify-between gap-2">
                      <span class="truncate font-medium text-foreground">
                        {claim.settingName || `0x${claim.settingId.toString(16)}`}
                      </span>
                      <span class="shrink-0 font-mono text-xs text-muted-foreground">
                        {claim.targetId} • {claim.observedLive?.present
                          ? t('game.card.removeConfirm.settingLive', {
                              value:
                                claim.observedLive.value ??
                                t('game.card.removeConfirm.settingPresent'),
                            })
                          : t('game.card.removeConfirm.settingAbsent')}
                      </span>
                    </li>
                  {/each}
                </ul>
              </ScrollArea>
            </div>
          {/if}
        {/if}

        {#if removeFlow.staleWarning}
          <Alert
            variant="warning"
            size="sm"
            role="status"
            class="mt-2 text-start"
            data-stale-warning
          >
            <TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
            <AlertDescription>
              {t('game.card.removeConfirm.staleWarning')}
            </AlertDescription>
          </Alert>
        {/if}

        {#if removeFlow.blockedReason !== null}
          <Alert
            variant="destructive"
            size="sm"
            role="alert"
            class="mt-2 text-start"
            data-removal-blocked
          >
            <TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
            <AlertTitle>{t('game.card.removeConfirm.blockedTitle')}</AlertTitle>
            <AlertDescription>{removeFlow.blockedReason}</AlertDescription>
          </Alert>
        {/if}

        {#if removeFlow.error !== null}
          <Alert
            variant="destructive"
            size="sm"
            role="alert"
            class="mt-2 text-start"
            data-removal-error
          >
            <AlertTitle>{t('notify.removeGameFailed')}</AlertTitle>
            <AlertDescription class="grid gap-2">
              <span>{removeFlow.error}</span>
              {#if removeFlow.recoveryBundlePath !== null}
                <code class="text-xs break-all">
                  {t('addGame.warning.recoveryBundleFallback', {
                    path: removeFlow.recoveryBundlePath,
                  })}
                </code>
              {/if}
              {#if !removeFlow.assessing && !removeFlow.submitting}
                <Button
                  variant="outline"
                  size="sm"
                  class="mt-1 w-fit"
                  onclick={removeFlow.retryAssessment}
                  data-retry-assessment
                >
                  {t('game.card.removeConfirm.reassessAction')}
                </Button>
              {/if}
            </AlertDescription>
          </Alert>
        {/if}
      </AlertDialogHeader>

      <AlertDialogFooter>
        <Button
          variant="secondary"
          size="sm"
          disabled={removeFlow.submitting}
          onclick={removeFlow.cancel}
        >
          {t('common.cancel')}
        </Button>
        <Button
          variant="destructive"
          size="sm"
          disabled={!removeFlow.canConfirm}
          onclick={removeFlow.confirm}
          data-removal-confirm
        >
          {#if removeFlow.submitting || removeFlow.assessing}
            <Spinner class="mr-1.5 size-4" />
          {:else}
            <Trash2Icon class="size-4" aria-hidden="true" />
          {/if}
          {removeFlow.isDiverged
            ? t('game.card.removeConfirm.abandonAction')
            : t('game.card.removeConfirm.action')}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
{/if}
