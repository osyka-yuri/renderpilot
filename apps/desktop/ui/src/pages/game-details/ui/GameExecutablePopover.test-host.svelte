<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import { TooltipProvider } from '@shared/ui';
  import type { ProfileSelectionBlockReason } from '../model/game-executable-lock';

  import GameExecutablePopover from './GameExecutablePopover.svelte';

  type Props = ComponentProps<typeof GameExecutablePopover>;

  const { ...props }: Props = $props();
  let profileSelectionBlockReason = $state<ProfileSelectionBlockReason | null>(null);

  export function setProfileSelectionBlockReason(reason: ProfileSelectionBlockReason | null): void {
    profileSelectionBlockReason = reason;
  }
</script>

<TooltipProvider delayDuration={0}>
  <GameExecutablePopover {...props} {profileSelectionBlockReason} />
</TooltipProvider>
