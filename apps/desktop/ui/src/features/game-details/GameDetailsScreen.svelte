<script lang="ts">
  import BackupOperationsPanel from '@features/game-details/components/BackupOperationsPanel.svelte';
  import GraphicsLibrariesConfigurator from '@features/game-details/components/GraphicsLibrariesConfigurator.svelte';
  import InstallContextCards from '@features/game-details/components/InstallContextCards.svelte';
  import OperationsHistoryPanel from '@features/game-details/components/OperationsHistoryPanel.svelte';
  import OperationPlanSummary from '@features/game-details/components/OperationPlanSummary.svelte';
  import {
    buildComponentRows,
    buildConfiguredRow,
    buildTechnologySections,
    buildVendorBlocks,
    reconcileArtifactSelections,
    reconcileNvapiSelections,
    sameSelectionMap,
    selectionKey,
    type VendorBlock,
    type VendorKey,
  } from '@features/game-details/lib/graphics-configurator';
  import type { GameCard, GameDetails, SwapPlan } from '@shared/api/types';
  import type { BuildPlanHandler, OperationHandler } from '@shared/utils/callbacks';
  import { formatTechnology } from '@shared/utils/presenters';
  import type { AccordionItem } from '@shared/ui/Accordion.svelte';

  const noopBuildPlan: BuildPlanHandler = (_componentId: string, _artifactId: string): void => {};
  const noopOperation: OperationHandler = (_operationId: string): void => {};

  export let details: GameDetails | null = null;
  export let gameCard: GameCard | null = null;
  export let plan: SwapPlan | null = null;
  export let busy = false;
  export let onBuildPlan: BuildPlanHandler = noopBuildPlan;
  export let onApply: OperationHandler = noopOperation;
  export let onRollback: OperationHandler = noopOperation;

  let selectedArtifacts: Record<string, string> = {};
  let selectedNvapiSelections: Record<string, string> = {};
  let activeVendorKey: VendorKey | null = null;
  let hasInteractedWithVendorAccordion = false;
  let lastVendorAccordionGameId: string | null = null;

  $: technologies = details
    ? [...new Set(details.components.map((component) => formatTechnology(component.technology)))]
    : [];
  $: backupOperations = details?.operations.filter((operation) => operation.backup_count > 0) ?? [];
  $: componentRows = details ? buildComponentRows(details) : [];
  $: configuredRows = componentRows.map((row) => buildConfiguredRow(row, selectedArtifacts, busy));
  $: technologySections = buildTechnologySections(configuredRows);
  $: vendorBlocks = buildVendorBlocks(technologySections);
  $: visibleVendorBlocks = vendorBlocks.filter((block) => block.key !== 'other' || block.sections.length > 0);
  $: vendorAccordionItems = visibleVendorBlocks.map(buildVendorAccordionItem);
  $: preferredVendorKey =
    visibleVendorBlocks.find((block) => block.sections.length > 0)?.key ?? visibleVendorBlocks[0]?.key ?? 'nvidia';
  $: currentVendorAccordionGameId = gameCard?.game_id ?? null;
  $: if (currentVendorAccordionGameId !== lastVendorAccordionGameId) {
    lastVendorAccordionGameId = currentVendorAccordionGameId;
    activeVendorKey = null;
    hasInteractedWithVendorAccordion = false;
  }
  $: effectiveVendorKey = hasInteractedWithVendorAccordion ? activeVendorKey : preferredVendorKey;

  $: {
    const nextArtifactSelections = details
      ? reconcileArtifactSelections(componentRows, selectedArtifacts, plan)
      : {};

    if (!sameSelectionMap(selectedArtifacts, nextArtifactSelections)) {
      selectedArtifacts = nextArtifactSelections;
    }
  }

  $: {
    const nextNvapiSelections = details
      ? reconcileNvapiSelections(componentRows, selectedNvapiSelections)
      : {};

    if (!sameSelectionMap(selectedNvapiSelections, nextNvapiSelections)) {
      selectedNvapiSelections = nextNvapiSelections;
    }
  }

  function vendorTechnologySummary(vendorBlock: VendorBlock): string {
    const labels = vendorBlock.sections.map((section) => section.label);

    if (labels.length === 0) {
      return 'No detected technologies yet.';
    }

    if (labels.length <= 2) {
      return labels.join(' · ');
    }

    return `${labels.slice(0, 2).join(' · ')} +${labels.length - 2} more`;
  }

  function buildVendorAccordionItem(vendorBlock: VendorBlock): AccordionItem {
    return {
      value: vendorBlock.key,
      title: vendorBlock.label,
      meta:
        vendorBlock.sections.length > 0
          ? `${vendorBlock.totalFiles} ${vendorBlock.totalFiles === 1 ? 'file' : 'files'}`
          : undefined,
      summary: vendorTechnologySummary(vendorBlock),
      badges:
        vendorBlock.sections.length > 0
          ? [
              {
                label: `${vendorBlock.sections.length} ${vendorBlock.sections.length === 1 ? 'technology' : 'technologies'}`,
              },
              {
                label: `${vendorBlock.totalCandidates} replacement ${vendorBlock.totalCandidates === 1 ? 'version' : 'versions'}`,
                tone: vendorBlock.totalCandidates > 0 ? 'success' : 'muted',
              },
            ]
          : [{ label: 'Empty', tone: 'muted' }],
    };
  }

  function handleArtifactSelection(componentId: string, nextValue: string): void {
    selectedArtifacts = {
      ...selectedArtifacts,
      [componentId]: nextValue,
    };
  }

  function handleNvapiSelection(componentId: string, controlId: string, nextValue: string): void {
    selectedNvapiSelections = {
      ...selectedNvapiSelections,
      [selectionKey(componentId, controlId)]: nextValue,
    };
  }

  function canRollback(status: string): boolean {
    return status === 'completed' || status === 'rollback_required';
  }

  function handleBuildPlan(componentId: string, artifactId: string): void {
    onBuildPlan(componentId, artifactId);
  }

  function handleRollback(operationId: string): void {
    onRollback(operationId);
  }

  function selectVendorTab(vendorKey: VendorKey | null): void {
    hasInteractedWithVendorAccordion = true;
    activeVendorKey = vendorKey;
  }
</script>

<section class="screen-shell">
  {#if !details}
    <div class="empty-state">
      <h3>No game selected</h3>
      <p>Select a game card on the dashboard to open one coherent workspace for that installation.</p>
    </div>
  {:else}
    <InstallContextCards
      installPath={details.game.install_path}
      launchCandidates={details.game.executable_candidates}
      {technologies}
    />

    <GraphicsLibrariesConfigurator
      vendorBlocks={visibleVendorBlocks}
      accordionItems={vendorAccordionItems}
      activeVendorKey={effectiveVendorKey}
      {selectedArtifacts}
      {selectedNvapiSelections}
      riskLevel={gameCard?.risk_level}
      {busy}
      {selectionKey}
      onVendorChange={selectVendorTab}
      onArtifactSelection={handleArtifactSelection}
      onNvapiSelection={handleNvapiSelection}
      onBuildPlan={handleBuildPlan}
    />

    <OperationPlanSummary {plan} {busy} onApply={onApply} />

    <section class="lower-grid">
      <BackupOperationsPanel operations={backupOperations} />
      <OperationsHistoryPanel
        operations={details.operations}
        {busy}
        {canRollback}
        onRollback={handleRollback}
      />
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
