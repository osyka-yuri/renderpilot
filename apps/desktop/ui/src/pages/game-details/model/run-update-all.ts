import { isMutationFailure, type AddonMutationResult } from '@entities/addon';
import { ClientError } from '@shared/errors';

import type { BulkSwapHandler } from './create-game-details-page-model';
import type { SwapRequest } from './swap-request';

type UpdateStore = {
  update(gameId: string): Promise<AddonMutationResult>;
};
export type UpdateAllStep = 'libraries' | 'renodx' | 'luma';

export type UpdateAllFailure = {
  step: UpdateAllStep;
  error: unknown;
};

export type RunUpdateAllOptions = {
  items: readonly SwapRequest[];
  gameId: string | null;
  addonUpdates: { step: Exclude<UpdateAllStep, 'libraries'>; store: UpdateStore }[];
  onBulkSwap: BulkSwapHandler;
};

export class UpdateAllError extends Error {
  readonly failures: UpdateAllFailure[];

  constructor(failures: UpdateAllFailure[]) {
    super('One or more update-all steps failed');
    this.name = 'UpdateAllError';
    this.failures = failures;
  }
}

/** Runs the captured update-all batch in a stable order. Unexpected failures
 * are isolated so one subsystem cannot prevent later add-ons from updating;
 * an aggregate error is reported after every eligible step was attempted.
 * Soft store skips (`busy`, no longer update-available) are not failures. */
export async function runUpdateAll({
  items,
  gameId,
  addonUpdates,
  onBulkSwap,
}: RunUpdateAllOptions): Promise<void> {
  const failures: UpdateAllFailure[] = [];

  if (items.length > 0) {
    try {
      await onBulkSwap(items);
    } catch (error) {
      failures.push({ step: 'libraries', error });
    }
  }

  if (gameId) {
    for (const { step, store } of addonUpdates) {
      try {
        // `update` returns a tri-state; only hard failures are aggregated.
        // Errors are usually notified inside the store and not rethrown.
        const result = await store.update(gameId);
        if (isMutationFailure(result)) {
          failures.push({ step, error: new ClientError('update_all_step_failed', { step }) });
        }
      } catch (error) {
        failures.push({ step, error });
      }
    }
  }

  if (failures.length > 0) {
    throw new UpdateAllError(failures);
  }
}
