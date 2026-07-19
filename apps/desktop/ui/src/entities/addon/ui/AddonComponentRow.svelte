<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    Item,
    ItemContent,
    ItemTitle,
    ItemDescription,
    ItemActions,
    ItemMedia,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
  import PuzzleIcon from '@lucide/svelte/icons/puzzle';
  import SparklesIcon from '@lucide/svelte/icons/sparkles';
  import InfoIcon from '@lucide/svelte/icons/info';

  import type { UpdateStatus } from '../model/types';
  import AddonToolStatusBadge from './AddonToolStatusBadge.svelte';
  import type { AddonToolI18nPrefix } from './types';

  const ICON_BY_KIND = {
    reshade: ShieldCheckIcon,
    addon: PuzzleIcon,
    dlssfix: SparklesIcon,
  } as const;

  type ItemIcon = keyof typeof ICON_BY_KIND;

  type Props = {
    icon: ItemIcon;
    title: string;
    description?: string | null;
    status?: UpdateStatus | null;
    /** Tool i18n prefix for freshness badges. Required when `status` is set. */
    statusI18nPrefix?: AddonToolI18nPrefix;
    hint?: string | null;
    actions?: Snippet;
  };

  let {
    icon,
    title,
    description = null,
    status = null,
    statusI18nPrefix,
    hint = null,
    actions,
  }: Props = $props();

  const Icon = $derived(ICON_BY_KIND[icon]);
  const hintText = $derived(hint?.trim() ?? null);
  const descriptionText = $derived(description?.trim() ?? null);
</script>

<Item size="sm">
  <ItemMedia>
    <Icon class="size-4 text-muted-foreground" aria-hidden="true" />
  </ItemMedia>

  <ItemContent>
    <ItemTitle>
      <span class="inline-flex items-center gap-1.5">
        {title}

        {#if hintText}
          <Tooltip>
            <TooltipTrigger
              class="inline-flex text-muted-foreground transition-colors hover:text-foreground"
              aria-label={hintText}
            >
              <InfoIcon class="size-3.5" aria-hidden="true" />
            </TooltipTrigger>

            <TooltipContent class="max-w-xs">
              {hintText}
            </TooltipContent>
          </Tooltip>
        {/if}
      </span>
    </ItemTitle>

    {#if descriptionText}
      <ItemDescription>{descriptionText}</ItemDescription>
    {/if}
  </ItemContent>

  <ItemActions>
    {#if status !== null && statusI18nPrefix}
      <AddonToolStatusBadge {status} i18nPrefix={statusI18nPrefix} />
    {/if}

    {@render actions?.()}
  </ItemActions>
</Item>
