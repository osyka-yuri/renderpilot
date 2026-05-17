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
  import { Card, CardContent, CardDescription, CardTitle, ScrollArea } from '@shared/ui';
  import { cn } from '@shared/classnames';

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
    onBuildPlan = () => undefined,
    onApply = () => undefined,
    onRollback = () => undefined,
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

<ScrollArea class="h-full min-h-0">
  <section class="grid gap-4">
    {#if !details}
      <Card>
        <CardContent>
          <CardTitle>No game selected</CardTitle>
          <CardDescription>
            Select a game card on the dashboard to open one coherent workspace for that
            installation.
          </CardDescription>
        </CardContent>
      </Card>
    {:else}
      <div class="grid gap-4">
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
          class={cn(
            'grid grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)] gap-4',
            'max-xl:grid-cols-1',
          )}
        >
          <BackupOperationsPanel operations={model.backupOperations} />

          <OperationsHistoryPanel
            operations={details.operations}
            {busy}
            {canRollback}
            {onRollback}
          />
        </section>
      </div>
    {/if}
  </section>
</ScrollArea>
