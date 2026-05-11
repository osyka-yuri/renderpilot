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

  let {
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

<section class="screen-shell">
  {#if !details}
    <div class="empty-state">
      <h3>No game selected</h3>
      <p>
        Select a game card on the dashboard to open one coherent workspace for that installation.
      </p>
    </div>
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

    <section class="lower-grid">
      <BackupOperationsPanel operations={model.backupOperations} />

      <OperationsHistoryPanel operations={details.operations} {busy} {canRollback} {onRollback} />
    </section>
  {/if}
</section>

<style>
  .screen-shell {
    display: grid;
    gap: var(--space-5);
  }

  .empty-state {
    padding: var(--space-4);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-xl);
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    box-shadow: none;
    color: var(--text-muted);
  }

  .empty-state h3 {
    margin: 0;
    color: var(--text-strong);
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .empty-state p {
    margin: var(--space-1) 0 0;
    color: var(--text-muted);
  }

  .lower-grid {
    display: grid;
    grid-template-columns: minmax(0, 0.95fr) minmax(0, 1.05fr);
    gap: var(--space-4);
  }

  @media (max-width: 1180px) {
    .lower-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
