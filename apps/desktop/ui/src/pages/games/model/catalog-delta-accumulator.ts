import type { CatalogDelta, CatalogDeltaReason } from '@entities/game';

export class CatalogDeltaAccumulator {
  readonly #reasons = new Set<CatalogDeltaReason>();
  readonly #changedGameIds = new Set<string>();
  readonly #removedGameIds = new Set<string>();
  #lastAcceptedRevision = 0;

  accept(delta: CatalogDelta, currentCatalogRevision: number): boolean {
    if (
      !Number.isSafeInteger(delta.revision) ||
      delta.revision <= Math.max(currentCatalogRevision, this.#lastAcceptedRevision)
    ) {
      return false;
    }

    this.#lastAcceptedRevision = delta.revision;
    for (const reason of delta.reasons) {
      this.#reasons.add(reason);
    }
    for (const gameId of normalizeGameIds(delta.changedGameIds)) {
      if (!this.#removedGameIds.has(gameId)) {
        this.#changedGameIds.add(gameId);
      }
    }
    for (const gameId of normalizeGameIds(delta.removedGameIds)) {
      this.#changedGameIds.delete(gameId);
      this.#removedGameIds.add(gameId);
    }
    return true;
  }

  reconcile(catalogRevision: number): void {
    if (catalogRevision < this.#lastAcceptedRevision) {
      return;
    }
    this.#lastAcceptedRevision = catalogRevision;
    this.#reasons.clear();
    this.#changedGameIds.clear();
    this.#removedGameIds.clear();
  }

  pending(currentCatalogRevision: number): CatalogDelta | null {
    if (this.#lastAcceptedRevision <= currentCatalogRevision) {
      return null;
    }
    return {
      revision: this.#lastAcceptedRevision,
      reasons: [...this.#reasons],
      changedGameIds: [...this.#changedGameIds],
      removedGameIds: [...this.#removedGameIds],
    };
  }
}

function normalizeGameIds(gameIds: readonly string[]): string[] {
  return [...new Set(gameIds.map((gameId) => gameId.trim()).filter(Boolean))];
}
