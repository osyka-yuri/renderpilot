import type { MutationSafetyScope, MutationSafetyTokens } from './types';

export type MutationSafetyCapture =
  | { kind: 'ready'; tokens: MutationSafetyTokens | undefined }
  | { kind: 'cancelled' };

/** Requests the safety context required by one add-on mutation. */
export async function requestMutationSafetyTokens<Scope extends MutationSafetyScope>(
  requester: ((gameId: string, scope: Scope) => Promise<MutationSafetyTokens | null>) | undefined,
  gameId: string,
  scope: Scope,
): Promise<MutationSafetyCapture> {
  // Preserve the same async boundary when no safety gate is configured.
  const tokens = await requester?.(gameId, scope);
  if (!requester) {
    return { kind: 'ready', tokens: undefined };
  }

  return tokens ? { kind: 'ready', tokens } : { kind: 'cancelled' };
}
