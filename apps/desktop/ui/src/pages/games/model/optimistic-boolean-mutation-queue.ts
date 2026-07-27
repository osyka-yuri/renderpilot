import type { GameSummary } from '@entities/game';

export type OptimisticBooleanMutation = {
  confirmedValue: boolean;
  optimisticValue: boolean;
  card: GameSummary;
  index: number;
  requestKey: string | null;
};

export function createOptimisticBooleanMutationQueue() {
  let sequence = 0;
  const latestTokens = new Map<string, number>();
  const mutations = new Map<string, OptimisticBooleanMutation>();
  const tails = new Map<string, Promise<unknown>>();

  function begin(
    gameId: string,
    card: GameSummary | null,
    index: number,
    requestKey: string | null,
    confirmedValue: boolean | undefined,
    optimisticValue: boolean,
  ): {
    token: number;
    mutation: OptimisticBooleanMutation | null;
    previousOptimisticValue: boolean | undefined;
  } {
    const token = ++sequence;
    latestTokens.set(gameId, token);

    let mutation = mutations.get(gameId) ?? null;
    if (mutation === null && card !== null && confirmedValue !== undefined) {
      mutation = {
        confirmedValue,
        optimisticValue: confirmedValue,
        card,
        index,
        requestKey,
      };
      mutations.set(gameId, mutation);
    }
    const previousOptimisticValue = mutation?.optimisticValue ?? confirmedValue;
    if (mutation !== null) {
      mutation.optimisticValue = optimisticValue;
    }

    return { token, mutation, previousOptimisticValue };
  }

  function isLatest(gameId: string, token: number): boolean {
    return latestTokens.get(gameId) === token;
  }

  function enqueue<TResult>(gameId: string, persist: () => Promise<TResult>): Promise<TResult> {
    const tail = tails.get(gameId);
    const operation = tail ? tail.catch(() => undefined).then(persist) : persist();
    tails.set(gameId, operation);
    return operation;
  }

  function patchCard(gameId: string, patch: Partial<GameSummary>): void {
    const mutation = mutations.get(gameId);
    if (mutation !== undefined) {
      mutation.card = { ...mutation.card, ...patch };
    }
  }

  function finish(gameId: string, operation: Promise<unknown>): void {
    if (tails.get(gameId) !== operation) {
      return;
    }
    tails.delete(gameId);
    mutations.delete(gameId);
    latestTokens.delete(gameId);
  }

  return { begin, isLatest, enqueue, patchCard, finish };
}
