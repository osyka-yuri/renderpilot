import type { CatalogDelta, CatalogSyncState, GameCardsResult, GameSummary } from '@entities/game';

import { CatalogDeltaAccumulator } from './catalog-delta-accumulator';
import { appendUniqueGameCards } from './games-catalog-items';
import type { GamesQuerySnapshot } from './games-page-query-scheduler';

/** Owns the mutable catalog/query state shared by the games page workflows. */
export class GamesCatalogSessionState {
  games = $state<readonly GameSummary[]>([]);
  catalogSize = $state(0);
  hiddenCount = $state(0);
  catalogRevision = $state(0);
  nextOffset = $state<number | null>(null);
  currentQuery = $state<GamesQuerySnapshot | null>(null);
  bootstrapping = $state(true);
  syncState = $state<CatalogSyncState>('refreshing');
  requestVersion = $state(0);

  readonly #deltas = new CatalogDeltaAccumulator();
  #suppressNextQuery = false;

  considerReactiveQuery(query: GamesQuerySnapshot): boolean {
    this.currentQuery = query;
    if (!this.#suppressNextQuery) {
      return true;
    }
    this.#suppressNextQuery = false;
    return false;
  }

  beginRefresh(): number {
    this.requestVersion += 1;
    this.#suppressNextQuery = false;
    return this.requestVersion;
  }

  replaceItems(items: readonly GameSummary[]): void {
    this.games = items;
  }

  appendItems(items: readonly GameSummary[]): void {
    this.games = appendUniqueGameCards(this.games, items);
  }

  patchCard(gameId: string, patch: Partial<GameSummary>): void {
    this.games = this.games.map((game) => (game.game_id === gameId ? { ...game, ...patch } : game));
  }

  removeCard(gameId: string): void {
    this.games = this.games.filter((game) => game.game_id !== gameId);
  }

  insertCard(card: GameSummary, index: number): void {
    const insertionIndex = Math.min(Math.max(index, 0), this.games.length);
    this.games = [
      ...this.games.slice(0, insertionIndex),
      card,
      ...this.games.slice(insertionIndex),
    ];
  }

  sortCards(compare: (left: GameSummary, right: GameSummary) => number): void {
    this.games = [...this.games].sort(compare);
  }

  setCatalogRevision(revision: number): void {
    this.catalogRevision = revision;
    this.#deltas.reconcile(revision);
  }

  applyBootstrap(result: GameCardsResult): void {
    this.games = result.items;
    this.catalogSize = result.catalogSize;
    this.hiddenCount = result.hiddenCount;
    this.setCatalogRevision(result.catalogRevision);
    this.nextOffset = result.nextOffset;
    this.#suppressNextQuery = true;
    this.bootstrapping = false;
    this.syncState = result.catalogSize === 0 ? 'refreshing' : 'ready';
  }

  completeBootstrapRecovery(): void {
    this.bootstrapping = false;
  }

  completeInitialSync(): void {
    this.syncState = 'ready';
  }

  acceptDelta(delta: CatalogDelta): boolean {
    return this.#deltas.accept(delta, this.catalogRevision);
  }

  pendingDelta(): CatalogDelta | null {
    return this.#deltas.pending(this.catalogRevision);
  }
}
