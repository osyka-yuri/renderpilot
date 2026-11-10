<script lang="ts">
  import type { ComponentProps } from 'svelte';

  import type { GameDetails } from '@entities/game';
  import { createLumaStore } from '@features/luma';
  import { createOptiScalerStore } from '@features/optiscaler';
  import { createRenoDxStore } from '@features/renodx';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { SwapHandler } from '../model/create-game-details-page-model';
  import { createNvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';
  import { createGameDetailsTabs } from '../model/game-details-tabs';
  import { Tabs, TooltipProvider } from '@shared/ui';

  import GameDetailsTabsContent from './GameDetailsTabsContent.svelte';

  type ContentProps = ComponentProps<typeof GameDetailsTabsContent>;
  type Props = {
    details: GameDetails;
    nvidia: NvidiaDriverContext;
    onSwap: SwapHandler;
  };

  const { details, nvidia, onSwap }: Props = $props();
  const tabs = $derived(createGameDetailsTabs(details));
  const noOp = () => undefined;
  const profile = createNvapiProfileContext();
  const renodx = createRenoDxStore();
  const luma = createLumaStore();
  const optiscaler = createOptiScalerStore();

  const contentProps = $derived({
    details,
    gameId: details.game.identity.id,
    installPath: details.game.install_path,
    profile,
    profileMutationBlocked: false,
    onOpenGameDetails: noOp,
    onRecoveryDeleteComplete: noOp,
    vendorTabs: tabs.vendorTabs,
    hasAddonsTab: false,
    nvidia,
    driverMutationBlocked: false,
    busy: false,
    exclusiveBusy: false,
    launcher: details.game.identity.launcher,
    renodx,
    luma,
    optiscaler,
    renodxEnabled: false,
    lumaEnabled: false,
    optiscalerEnabled: false,
    onSwap,
    onRollback: noOp,
    onBulkSwap: noOp,
    onBulkRollback: noOp,
    onOpenRenoDxSettings: noOp,
    onPreloadRenoDxSettings: noOp,
  } satisfies ContentProps);
</script>

<TooltipProvider delayDuration={0}>
  <Tabs value="nvidia">
    <GameDetailsTabsContent {...contentProps} />
  </Tabs>
</TooltipProvider>
