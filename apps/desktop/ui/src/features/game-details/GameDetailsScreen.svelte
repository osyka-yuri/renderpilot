<script lang="ts">
  import BackupOperationsPanel from '@features/game-details/components/BackupOperationsPanel.svelte';
  import GraphicsLibrariesConfigurator from '@features/game-details/components/GraphicsLibrariesConfigurator.svelte';
  import InstallContextCards from '@features/game-details/components/InstallContextCards.svelte';
  import OperationsHistoryPanel from '@features/game-details/components/OperationsHistoryPanel.svelte';
  import OperationPlanSummary from '@features/game-details/components/OperationPlanSummary.svelte';
  import {
    buildComponentRows,
    buildConfiguredRow,
    buildLibrarySections,
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
  import { formatLibrary } from '@shared/utils/presenters';
  import type { AccordionItem } from '@shared/ui/Accordion.svelte';

  type SelectionMap = Record<string, string>;

  type VendorAccordionState = {
    gameId: string | null;
    activeVendorKey: VendorKey | null;
    hasSelectedVendorManually: boolean;
  };

  const DEFAULT_VENDOR_KEY: VendorKey = 'nvidia';

  const noopBuildPlan: BuildPlanHandler = () => undefined;
  const noopOperation: OperationHandler = () => undefined;

  type Props = {
    details?: GameDetails | null;
    gameCard?: GameCard | null;
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

  let selectedArtifacts = $state<SelectionMap>({});
  let selectedNvapiSelections = $state<SelectionMap>({});

  let vendorAccordionState = $state<VendorAccordionState>(createVendorAccordionState(null));

  const currentGameId = $derived(gameCard?.game_id ?? null);

  $effect.pre(() => {
    if (currentGameId !== vendorAccordionState.gameId) {
      vendorAccordionState = createVendorAccordionState(currentGameId);
    }
  });

  const libraries = $derived(details ? getLibraries(details) : []);
  const backupOperations = $derived(details?.operations.filter(hasBackups) ?? []);

  const componentRows = $derived(details ? buildComponentRows(details) : []);

  $effect.pre(() => {
    const reconciledArtifacts = details
      ? reconcileArtifactSelections(componentRows, selectedArtifacts, plan)
      : {};

    if (!sameSelectionMap(selectedArtifacts, reconciledArtifacts)) {
      selectedArtifacts = reconciledArtifacts;
    }
  });

  $effect.pre(() => {
    const reconciledNvapiSelections = details
      ? reconcileNvapiSelections(componentRows, selectedNvapiSelections)
      : {};

    if (!sameSelectionMap(selectedNvapiSelections, reconciledNvapiSelections)) {
      selectedNvapiSelections = reconciledNvapiSelections;
    }
  });

  const configuredRows = $derived(
    componentRows.map((row) => buildConfiguredRow(row, selectedArtifacts, busy)),
  );

  const librarySections = $derived(buildLibrarySections(configuredRows));
  const vendorBlocks = $derived(buildVendorBlocks(librarySections));
  const visibleVendorBlocks = $derived(vendorBlocks.filter(hasVisibleVendorContent));
  const vendorAccordionItems = $derived(visibleVendorBlocks.map(buildVendorAccordionItem));

  const preferredVendorKey = $derived(resolvePreferredVendorKey(visibleVendorBlocks));

  const activeVendorKeyIsUsable = $derived(
    isActiveVendorKeyUsable(visibleVendorBlocks, vendorAccordionState.activeVendorKey),
  );

  const effectiveVendorKey = $derived(
    vendorAccordionState.hasSelectedVendorManually && activeVendorKeyIsUsable
      ? vendorAccordionState.activeVendorKey
      : preferredVendorKey,
  );

  function createVendorAccordionState(gameId: string | null): VendorAccordionState {
    return {
      gameId,
      activeVendorKey: null,
      hasSelectedVendorManually: false,
    };
  }

  function getLibraries(gameDetails: GameDetails): string[] {
    return [
      ...new Set(gameDetails.components.map((component) => formatLibrary(component.technology))),
    ];
  }

  function hasBackups(operation: GameDetails['operations'][number]): boolean {
    return operation.backup_count > 0;
  }

  function hasVisibleVendorContent(block: VendorBlock): boolean {
    return block.key !== 'other' || block.sections.length > 0;
  }

  function hasVendorKey(blocks: VendorBlock[], vendorKey: VendorKey): boolean {
    return blocks.some((block) => block.key === vendorKey);
  }

  function isActiveVendorKeyUsable(blocks: VendorBlock[], vendorKey: VendorKey | null): boolean {
    return vendorKey === null || hasVendorKey(blocks, vendorKey);
  }

  function resolvePreferredVendorKey(blocks: VendorBlock[]): VendorKey {
    const populatedBlock = blocks.find((block) => block.sections.length > 0);

    if (populatedBlock !== undefined) {
      return populatedBlock.key;
    }

    if (blocks.length > 0) {
      return blocks[0].key;
    }

    return DEFAULT_VENDOR_KEY;
  }

  function vendorLibrarySummary(vendorBlock: VendorBlock): string {
    const labels = vendorBlock.sections.map((section) => section.label);

    if (labels.length === 0) {
      return 'No detected libraries yet.';
    }

    if (labels.length <= 2) {
      return labels.join(' · ');
    }

    return `${labels.slice(0, 2).join(' · ')} +${labels.length - 2} more`;
  }

  function buildVendorAccordionItem(vendorBlock: VendorBlock): AccordionItem {
    const hasSections = vendorBlock.sections.length > 0;

    return {
      value: vendorBlock.key,
      title: vendorBlock.label,
      meta: hasSections ? formatFileCount(vendorBlock.totalFiles) : undefined,
      summary: vendorLibrarySummary(vendorBlock),
      badges: hasSections
        ? [
            {
              label: formatLibraryCount(vendorBlock.sections.length),
            },
            {
              label: formatReplacementCount(vendorBlock.totalCandidates),
              tone: vendorBlock.totalCandidates > 0 ? 'success' : 'muted',
            },
          ]
        : [{ label: 'Empty', tone: 'muted' }],
    };
  }

  function formatFileCount(count: number): string {
    return `${count} ${count === 1 ? 'file' : 'files'}`;
  }

  function formatLibraryCount(count: number): string {
    return `${count} ${count === 1 ? 'library' : 'libraries'}`;
  }

  function formatReplacementCount(count: number): string {
    return `${count} replacement ${count === 1 ? 'version' : 'versions'}`;
  }

  function updateSelection(map: SelectionMap, key: string, value: string): SelectionMap {
    return {
      ...map,
      [key]: value,
    };
  }

  function handleArtifactSelection(componentId: string, artifactId: string): void {
    selectedArtifacts = updateSelection(selectedArtifacts, componentId, artifactId);
  }

  function handleNvapiSelection(componentId: string, controlId: string, artifactId: string): void {
    selectedNvapiSelections = updateSelection(
      selectedNvapiSelections,
      selectionKey(componentId, controlId),
      artifactId,
    );
  }

  function handleVendorChange(vendorKey: VendorKey | null): void {
    vendorAccordionState = {
      ...vendorAccordionState,
      activeVendorKey: vendorKey,
      hasSelectedVendorManually: true,
    };
  }

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
      {libraries}
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
      onVendorChange={handleVendorChange}
      onArtifactSelection={handleArtifactSelection}
      onNvapiSelection={handleNvapiSelection}
      {onBuildPlan}
    />

    <OperationPlanSummary {plan} {busy} {onApply} />

    <section class="lower-grid">
      <BackupOperationsPanel operations={backupOperations} />

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
