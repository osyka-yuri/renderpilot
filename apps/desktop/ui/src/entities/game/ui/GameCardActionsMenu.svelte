<script lang="ts">
  import EllipsisIcon from '@lucide/svelte/icons/ellipsis';

  import { cn } from '@shared/classnames';
  import { t } from '@shared/i18n';
  import { Button, Popover, PopoverContent, PopoverTrigger, buttonVariants } from '@shared/ui';

  type MenuActionId = 'fetch-cover' | 'pick-cover' | 'clear-cover';

  type MenuActionHandler = () => void | Promise<void>;

  type MenuAction = {
    readonly id: MenuActionId;
    readonly label: string;
    readonly title: string;
    readonly disabled: boolean;
    readonly danger?: boolean;
    readonly handler: MenuActionHandler;
  };

  type Props = {
    title: string;
    disabled?: boolean;
    pickDisabled?: boolean;
    autoFetchInProgress?: boolean;
    hasCover?: boolean;
    open?: boolean;

    onOpenChange?: (next: boolean) => void;
    onFetchCover?: MenuActionHandler;
    onPickCover?: MenuActionHandler;
    onClearCover?: MenuActionHandler;
  };

  const noop: MenuActionHandler = () => undefined;
  const noopOpenChange = () => undefined;

  let {
    title,
    disabled = false,
    pickDisabled = false,
    autoFetchInProgress = false,
    hasCover = false,
    open = false,

    onOpenChange = noopOpenChange,
    onFetchCover = noop,
    onPickCover = noop,
    onClearCover = noop,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);

  const isMenuOpen = $derived(open && !disabled);
  const coverOptionsLabel = $derived(t('game.cover.menu.ariaLabel', { title }));

  const isFetchCoverDisabled = $derived(disabled || autoFetchInProgress);
  const isPickCoverDisabled = $derived(disabled || pickDisabled);
  const isClearCoverDisabled = $derived(disabled || !hasCover);

  const menuActions = $derived.by((): MenuAction[] => [
    {
      id: 'fetch-cover',
      label: autoFetchInProgress ? t('game.cover.menu.fetching') : t('game.cover.menu.fetch'),
      disabled: isFetchCoverDisabled,
      title: t('game.cover.menu.fetchHint'),
      handler: onFetchCover,
    },
    {
      id: 'pick-cover',
      label: t('game.cover.menu.pick'),
      disabled: isPickCoverDisabled,
      title: t('game.cover.menu.pickHint'),
      handler: onPickCover,
    },
    {
      id: 'clear-cover',
      label: t('game.cover.menu.clear'),
      disabled: isClearCoverDisabled,
      danger: true,
      title: t('game.cover.menu.clearHint'),
      handler: onClearCover,
    },
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
      console.error(`Cover menu action "${action.id}" failed.`, error);
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
    aria-label={coverOptionsLabel}
    aria-haspopup="menu"
    {disabled}
  >
    <EllipsisIcon class="block size-4 shrink-0" aria-hidden="true" />
  </PopoverTrigger>

  <PopoverContent align="end" sideOffset={6}>
    <div role="menu" aria-label={coverOptionsLabel} class="grid gap-1">
      {#each menuActions as action (action.id)}
        <Button
          variant={action.danger === true ? 'destructive' : 'ghost'}
          size="sm"
          class={cn('w-full justify-start text-left', action.danger === true && 'justify-between')}
          role="menuitem"
          disabled={action.disabled}
          title={action.title}
          onclick={() => {
            handleMenuActionClick(action);
          }}
        >
          {action.label}
        </Button>
      {/each}
    </div>
  </PopoverContent>
</Popover>
