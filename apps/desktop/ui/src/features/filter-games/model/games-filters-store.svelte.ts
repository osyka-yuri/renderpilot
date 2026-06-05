import {
  applyDraftFilters,
  cancelFilterDialog,
  createInitialGamesFilterState,
  openFilterDialog,
  setDraftLibraries,
  setDraftLaunchers,
  setDraftLauncherOrder,
  setDraftShowHidden,
  setDraftFavoritesOnly,
  toggleAppliedFavoritesOnly,
  toggleAppliedShowHidden,
  withSearchQuery,
  type GamesFilterState,
} from './index-internal';
import { canonicalizeLauncherOrder } from './launcher-order';

export class GamesFiltersStore {
  state = $state(createInitialGamesFilterState());

  setState(nextState: GamesFilterState) {
    this.state = nextState;
  }

  handleDialogOpenChange(nextOpen: boolean): void {
    if (nextOpen) {
      this.state = openFilterDialog(this.state);
      return;
    }
    this.state = cancelFilterDialog(this.state);
  }

  applyFilterSelection(): void {
    this.state = applyDraftFilters(this.state);
  }

  cancelFilterSelection(): void {
    this.state = cancelFilterDialog(this.state);
  }

  toggleFiltersDialog(): void {
    this.handleDialogOpenChange(!this.state.isDialogOpen);
  }

  handleDraftLibrariesChange(nextLibraries: readonly string[]): void {
    this.state = setDraftLibraries(this.state, nextLibraries);
  }

  handleDraftLaunchersChange(nextLaunchers: readonly string[]): void {
    this.state = setDraftLaunchers(this.state, nextLaunchers);
  }

  handleDraftLauncherOrderChange(nextOrder: readonly string[]): void {
    this.state = setDraftLauncherOrder(this.state, nextOrder);
  }

  resetFilters(): void {
    let next = withSearchQuery(this.state, '');
    next = setDraftLibraries(next, next.availableLibraries);
    next = setDraftLaunchers(next, next.availableLaunchers);
    next = setDraftLauncherOrder(next, canonicalizeLauncherOrder([], next.availableLaunchers));
    next = setDraftShowHidden(next, false);
    next = setDraftFavoritesOnly(next, false);

    this.state = applyDraftFilters(next);
  }

  setSearchQuery(nextValue: string): void {
    this.state = withSearchQuery(this.state, nextValue);
  }

  quickToggleFavoritesOnly(): void {
    this.state = toggleAppliedFavoritesOnly(this.state);
  }

  quickToggleShowHidden(): void {
    this.state = toggleAppliedShowHidden(this.state);
  }
}
