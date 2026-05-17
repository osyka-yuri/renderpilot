<script lang="ts">
  import { Button, Tooltip, TooltipContent, TooltipTrigger } from '@shared/ui';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';

  type Props = {
    refreshing: boolean;
    disabled: boolean;
    onRefresh: () => void;
  };

  const { refreshing, disabled, onRefresh }: Props = $props();
</script>

<Tooltip>
  <TooltipTrigger>
    <Button
      variant="outline"
      size="icon"
      class="size-9"
      onclick={onRefresh}
      {disabled}
      aria-label="Refresh manifest"
    >
      <RefreshCwIcon class={refreshing ? 'animate-spin' : ''} aria-hidden="true" />
    </Button>
  </TooltipTrigger>

  <TooltipContent>Refresh manifest</TooltipContent>
</Tooltip>
