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

  import RenoDxStatusBadge from './RenoDxStatusBadge.svelte';
  import type { UpdateStatus } from '../model/types';

  // One row in the installed-state component list: the ReShade host, the RenoDX
  // add-on, or the DLSS-Fix companion. `icon` is a small enum so callers never
  // pass a component; `status` renders a freshness pill when set; `hint` adds an
  // info tooltip after the title (progressive disclosure for a longer "how it
  // works" note); `actions` is an optional trailing snippet (a button).
  type Props = {
    icon: 'reshade' | 'addon' | 'dlssfix';
    title: string;
    description: string;
    status?: UpdateStatus | null;
    hint?: string;
    actions?: Snippet;
  };

  const { icon, title, description, status = null, hint, actions }: Props = $props();
</script>

<Item size="sm">
  <ItemMedia>
    {#if icon === 'reshade'}
      <ShieldCheckIcon class="size-4 text-muted-foreground" aria-hidden="true" />
    {:else if icon === 'addon'}
      <PuzzleIcon class="size-4 text-muted-foreground" aria-hidden="true" />
    {:else}
      <SparklesIcon class="size-4 text-muted-foreground" aria-hidden="true" />
    {/if}
  </ItemMedia>
  <ItemContent>
    <ItemTitle>
      <span class="inline-flex items-center gap-1.5">
        {title}
        {#if hint}
          <Tooltip>
            <TooltipTrigger
              class="text-muted-foreground hover:text-foreground inline-flex"
              aria-label={hint}
            >
              <InfoIcon class="size-3.5" aria-hidden="true" />
            </TooltipTrigger>
            <TooltipContent class="max-w-xs">{hint}</TooltipContent>
          </Tooltip>
        {/if}
      </span>
    </ItemTitle>
    <ItemDescription>{description}</ItemDescription>
  </ItemContent>
  <ItemActions>
    {#if status}
      <RenoDxStatusBadge {status} />
    {/if}
    {@render actions?.()}
  </ItemActions>
</Item>
