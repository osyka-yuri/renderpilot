import type { GameCandidate } from './types';

/** Reads the backend-computed unattended-selection capability. */
export function isAutomaticCatalogCandidate(candidate: GameCandidate): boolean {
  return candidate.catalog_package?.automatic_selection_allowed === true;
}
