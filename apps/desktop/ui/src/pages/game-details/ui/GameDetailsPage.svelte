<script lang="ts">
  import { selectionKey } from '@features/graphics-configurator';
  import type { GameDetails, GameSummary } from '@entities/game';
  import { type SwapPlan } from '@entities/operation';
  import type { BuildPlanHandler } from '@entities/component';
  import type { OperationHandler } from '@entities/operation';
  import {
    BackupOperationsPanel,
    createGraphicsLibrariesModel,
    GraphicsLibrariesConfigurator,
    InstallContextCards,
    OperationPlanSummary,
    OperationsHistoryPanel,
  } from '@widgets/game-workspace';
  import { EmptyStatePanel } from '@shared/ui';
  import { cn } from '@shared/utils';

  const noopBuildPlan: BuildPlanHandler = () => undefined;
  const noopOperation: OperationHandler = () => undefined;

  type Props = {
    details?: GameDetails | null;
    gameCard?: GameSummary | null;
    plan?: SwapPlan | null;
    busy?: boolean;
    onBuildPlan?: BuildPlanHandler;
    onApply?: OperationHandler;
    onRollback?: OperationHandler;
  };

  const {
    details = null,
    gameCard = null,
    plan = null,
    busy = false,
    onBuildPlan = noopBuildPlan,
    onApply = noopOperation,
    onRollback = noopOperation,
  }: Props = $props();

  const model = createGraphicsLibrariesModel({
    getDetails: () => details,
    getGameCard: () => gameCard,
    getPlan: () => plan,
    getBusy: () => busy,
  });

  function canRollback(status: string): boolean {
    return status === 'completed' || status === 'rollback_required';
  }
</script>

<section class="grid gap-5">
  {#if !details}
    <EmptyStatePanel>
      <h3 class="text-base/tight font-semibold text-text-strong">No game selected</h3>
      <p class="mt-1 text-text-muted">
        Select a game card on the dashboard to open one coherent workspace for that installation.
      </p>
    </EmptyStatePanel>
  {:else}
    <InstallContextCards
      installPath={details.game.install_path}
      launchCandidates={details.game.executable_candidates}
      libraries={model.libraries}
    />

    <GraphicsLibrariesConfigurator
      vendorBlocks={model.visibleVendorBlocks}
      accordionItems={model.vendorAccordionItems}
      activeVendorKey={model.effectiveVendorKey}
      selectedArtifacts={model.selectedArtifacts}
      selectedNvapiSelections={model.selectedNvapiSelections}
      riskLevel={gameCard?.risk_level}
      {busy}
      {selectionKey}
      onVendorChange={model.handleVendorChange}
      onArtifactSelection={model.handleArtifactSelection}
      onNvapiSelection={model.handleNvapiSelection}
      {onBuildPlan}
    />

    <OperationPlanSummary {plan} {busy} {onApply} />

    <section
      class={cn('grid grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)] gap-4', 'max-xl:grid-cols-1')}
    >
      <BackupOperationsPanel operations={model.backupOperations} />

      <OperationsHistoryPanel operations={details.operations} {busy} {canRollback} {onRollback} />
    </section>
  {/if}
</section>
