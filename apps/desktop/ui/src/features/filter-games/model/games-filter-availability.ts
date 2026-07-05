import { shallowStringArrayEqual } from '@shared/text';
import { canonicalizeLauncherOrder } from './launcher-order';
import type { GamesFilterState } from './games-filter-state';

/**
 * Reconciles the library and launcher availability state against the new incoming lists.
 * Preserves object identity if no changes occurred to prevent unnecessary re-renders.
 */
export function withAvailableSnapshots(
  state: GamesFilterState,
  availableLibraries: string[],
  availableLaunchers: string[],
  availableAddons: string[] = state.availableAddons,
): GamesFilterState {
  const librariesChanged = !shallowStringArrayEqual(state.availableLibraries, availableLibraries);
  const addonsChanged = !shallowStringArrayEqual(state.availableAddons, availableAddons);
  const launchersChanged = !shallowStringArrayEqual(state.availableLaunchers, availableLaunchers);

  if (!librariesChanged && !addonsChanged && !launchersChanged) {
    return state;
  }

  return {
    ...state,
    availableLibraries: librariesChanged ? availableLibraries : state.availableLibraries,
    availableAddons: addonsChanged ? availableAddons : state.availableAddons,
    availableLaunchers: launchersChanged ? availableLaunchers : state.availableLaunchers,
  };
}

/**
 * Reconciles both the applied and draft launcher sort orders against the current
 * list of available launchers. Ensures that removed launchers are purged and
 * new launchers are appended correctly.
 */
export function reconcileLauncherOrderWithAvailability(
  state: GamesFilterState,
  availableLaunchers: readonly string[],
): GamesFilterState {
  const appliedLauncherOrder = canonicalizeLauncherOrder(
    state.appliedLauncherOrder,
    availableLaunchers,
  );

  const draftLauncherOrder = state.isDialogOpen
    ? canonicalizeLauncherOrder(state.draftLauncherOrder, availableLaunchers)
    : [...appliedLauncherOrder];

  const appliedChanged = !shallowStringArrayEqual(state.appliedLauncherOrder, appliedLauncherOrder);

  const draftChanged = !shallowStringArrayEqual(state.draftLauncherOrder, draftLauncherOrder);

  if (!appliedChanged && !draftChanged) {
    return state;
  }

  return {
    ...state,
    appliedLauncherOrder: appliedChanged ? appliedLauncherOrder : state.appliedLauncherOrder,
    draftLauncherOrder: draftChanged ? draftLauncherOrder : state.draftLauncherOrder,
  };
}
