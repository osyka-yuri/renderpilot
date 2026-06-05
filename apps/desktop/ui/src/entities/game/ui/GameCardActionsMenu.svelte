<script lang="ts">
  import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
  import StarIcon from '@lucide/svelte/icons/star';
  import StarOffIcon from '@lucide/svelte/icons/star-off';
  import EyeIcon from '@lucide/svelte/icons/eye';
  import EyeOffIcon from '@lucide/svelte/icons/eye-off';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import type { Component } from 'svelte';

  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import {
    Button,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Separator,
    buttonVariants,
  } from '@shared/ui';

  type MenuActionId =
    | 'fetch-cover'
    | 'pick-cover'
    | 'clear-cover'
    | 'toggle-favorite'
    | 'toggle-hidden';

  type MenuActionHandler = () => void | Promise<void>;

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
    title: string;
    disabled?: boolean;
    pickDisabled?: boolean;
    autoFetchInProgress?: boolean;
    hasCover?: boolean;
    isFavorite?: boolean;
    isHidden?: boolean;
    open?: boolean;

    onOpenChange?: (next: boolean) => void;
    onFetchCover?: MenuActionHandler;
    onPickCover?: MenuActionHandler;
    onClearCover?: MenuActionHandler;
    onToggleFavorite?: MenuActionHandler;
    onToggleHidden?: MenuActionHandler;
  };

  const noop: MenuActionHandler = () => undefined;
  const noopOpenChange = () => undefined;

  let {
    title,
    disabled = false,
    pickDisabled = false,
    autoFetchInProgress = false,
    hasCover = false,
    isFavorite = false,
    isHidden = false,
    open = false,

    onOpenChange = noopOpenChange,
    onFetchCover = noop,
    onPickCover = noop,
    onClearCover = noop,
    onToggleFavorite = noop,
    onToggleHidden = noop,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);

  const isMenuOpen = $derived(open && !disabled);
  const gameOptionsLabel = $derived(t('game.card.menu.ariaLabel', { title }));

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
  ]);

  $effect(() => {
    if (disabled && open) {
      onOpenChange(false);
    }
  });

  export function focusTrigger(): void {
    triggerEl?.focus({ preventScroll: true });
  }

  function handlePopoverOpenChange(nextOpen: boolean): void {
    if (nextOpen && disabled) {
      return;
    }

    onOpenChange(nextOpen);
  }

  async function runMenuAction(action: MenuAction): Promise<void> {
    try {
      await action.handler();
    } catch (error) {
      console.error(`Game menu action "${action.id}" failed.`, error);
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

<Popover open={isMenuOpen} onOpenChange={handlePopoverOpenChange}>
  <PopoverTrigger
    bind:ref={triggerEl}
    class={buttonVariants({ variant: 'outline', size: 'icon-sm' })}
    aria-label={gameOptionsLabel}
    aria-haspopup="menu"
    {disabled}
  >
    <EllipsisIcon class="block size-4 shrink-0" aria-hidden="true" />
  </PopoverTrigger>

  <PopoverContent align="end" sideOffset={6}>
    <div role="menu" aria-label={gameOptionsLabel} class="grid gap-1">
      {#each menuActionGroups as group, i (i)}
        {#if i > 0}
          <Separator class="my-1" />
        {/if}
        {#each group as action (action.id)}
          <Button
            variant={action.danger === true ? 'destructive' : 'ghost'}
            size="sm"
            class={cn('w-full justify-start text-left gap-2')}
            role="menuitem"
            disabled={action.disabled}
            title={action.title}
            onclick={() => {
              handleMenuActionClick(action);
            }}
          >
            {@const Icon = action.icon}
            <Icon class="size-4 shrink-0" aria-hidden="true" />
            <span class="flex-1 truncate">{action.label}</span>
          </Button>
        {/each}
      {/each}
    </div>
  </PopoverContent>
</Popover>
