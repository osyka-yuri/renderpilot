import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import {
  buildComponentRows,
  createGraphicsConfiguratorViewModel,
  reconcileArtifactSelections,
  reconcileNvapiSelections,
  sameSelectionMap,
  updateArtifactSelection,
  updateNvapiSelection,
} from './graphics-configurator';

export type GraphicsConfiguratorSelectionMap = Record<string, string>;

export type GraphicsConfiguratorModel = ReturnType<typeof createGraphicsConfiguratorModel>;

export type GraphicsConfiguratorModelInput = {
  getDetails: () => GameDetails | null;
  getPlan: () => SwapPlan | null;
  getBusy: () => boolean;
};

export function createGraphicsConfiguratorModel(input: GraphicsConfiguratorModelInput) {
  const initialDetails = input.getDetails();
  const initialRows = initialDetails ? buildComponentRows(initialDetails) : [];

  let selectedArtifacts = $state<GraphicsConfiguratorSelectionMap>(
    initialDetails ? reconcileArtifactSelections(initialRows, {}, input.getPlan()) : {},
  );
  let selectedNvapiSelections = $state<GraphicsConfiguratorSelectionMap>(
    initialDetails ? reconcileNvapiSelections(initialRows, {}) : {},
  );

  const details = $derived(input.getDetails());
  const componentRows = $derived(details ? buildComponentRows(details) : []);

  $effect.pre(() => {
    const next = details
      ? reconcileArtifactSelections(componentRows, selectedArtifacts, input.getPlan())
      : {};

    if (!sameSelectionMap(selectedArtifacts, next)) {
      selectedArtifacts = next;
    }
  });

  $effect.pre(() => {
    const next = details ? reconcileNvapiSelections(componentRows, selectedNvapiSelections) : {};

    if (!sameSelectionMap(selectedNvapiSelections, next)) {
      selectedNvapiSelections = next;
    }
  });

  const viewModel = $derived(
    details
      ? createGraphicsConfiguratorViewModel(details, selectedArtifacts, input.getBusy())
      : null,
  );

  function handleArtifactSelection(componentId: string, artifactId: string): void {
    selectedArtifacts = updateArtifactSelection(selectedArtifacts, componentId, artifactId);
  }

  function handleNvapiSelection(componentId: string, controlId: string, artifactId: string): void {
    selectedNvapiSelections = updateNvapiSelection(
      selectedNvapiSelections,
      componentId,
      controlId,
      artifactId,
    );
  }

  return {
    get componentRows() {
      return componentRows;
    },
    get selectedArtifacts() {
      return selectedArtifacts;
    },
    get selectedNvapiSelections() {
      return selectedNvapiSelections;
    },
    get viewModel() {
      return viewModel;
    },
    handleArtifactSelection,
    handleNvapiSelection,
  };
}
