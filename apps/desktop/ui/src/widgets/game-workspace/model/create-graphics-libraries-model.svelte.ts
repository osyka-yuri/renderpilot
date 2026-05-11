import { createGraphicsConfiguratorModel, type VendorKey } from '@features/graphics-configurator';
import { formatLibrary } from '@entities/component';
import type { GameDetails, GameSummary } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import {
  createVendorAccordionState,
  handleVendorChange as createVendorChange,
  resolveEffectiveVendorKey,
  hasVisibleVendorContent,
  buildVendorAccordionItems,
  type VendorAccordionState,
} from './vendor-accordion';

export type GraphicsLibrariesModel = ReturnType<typeof createGraphicsLibrariesModel>;

export function createGraphicsLibrariesModel(options: {
  getDetails: () => GameDetails | null;
  getGameCard: () => GameSummary | null;
  getPlan: () => SwapPlan | null;
  getBusy: () => boolean;
}) {
  let vendorAccordionState = $state<VendorAccordionState>(createVendorAccordionState(null));

  const currentGameId = $derived(options.getGameCard()?.game_id ?? null);
  const configurator = createGraphicsConfiguratorModel({
    getDetails: options.getDetails,
    getPlan: options.getPlan,
    getBusy: options.getBusy,
  });

  $effect.pre(() => {
    if (currentGameId !== vendorAccordionState.gameId) {
      vendorAccordionState = createVendorAccordionState(currentGameId);
    }
  });

  const details = $derived(options.getDetails());

  const libraries = $derived(details ? getLibraries(details) : []);
  const backupOperations = $derived(details?.operations.filter(hasBackups) ?? []);

  const viewModel = $derived(configurator.viewModel);

  const visibleVendorBlocks = $derived(
    (viewModel?.vendorBlocks ?? []).filter(hasVisibleVendorContent),
  );
  const vendorAccordionItems = $derived(buildVendorAccordionItems(visibleVendorBlocks));
  const effectiveVendorKey = $derived(
    resolveEffectiveVendorKey(visibleVendorBlocks, vendorAccordionState),
  );

  function getLibraries(gameDetails: GameDetails): string[] {
    const result: string[] = [];
    for (const component of gameDetails.components) {
      const lib = formatLibrary(component.technology);
      if (!result.includes(lib)) {
        result.push(lib);
      }
    }
    return result;
  }

  function handleArtifactSelection(componentId: string, artifactId: string): void {
    configurator.handleArtifactSelection(componentId, artifactId);
  }

  function handleNvapiSelection(componentId: string, controlId: string, artifactId: string): void {
    configurator.handleNvapiSelection(componentId, controlId, artifactId);
  }

  function handleVendorChange(vendorKey: VendorKey | null): void {
    vendorAccordionState = createVendorChange(vendorAccordionState, vendorKey);
  }

  return {
    get libraries() {
      return libraries;
    },
    get backupOperations() {
      return backupOperations;
    },
    get visibleVendorBlocks() {
      return visibleVendorBlocks;
    },
    get vendorAccordionItems() {
      return vendorAccordionItems;
    },
    get effectiveVendorKey() {
      return effectiveVendorKey;
    },
    get selectedArtifacts() {
      return configurator.selectedArtifacts;
    },
    get selectedNvapiSelections() {
      return configurator.selectedNvapiSelections;
    },
    handleArtifactSelection,
    handleNvapiSelection,
    handleVendorChange,
  };
}

function hasBackups(operation: GameDetails['operations'][number]): boolean {
  return operation.backup_count > 0;
}
