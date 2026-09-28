import { isMutationFailure, type AddonMutationResult } from '@entities/addon';
import { ClientError, isFileSafetyContextError } from '@shared/errors';

import type { BulkSwapHandler } from './create-game-details-page-model';
import type { SwapRequest } from './swap-request';

type UpdateStore = {
  update(gameId: string): Promise<AddonMutationResult>;
  safetyContextError?: unknown;
};
export type UpdateAllStep = 'libraries' | 'renodx' | 'luma' | 'optiscaler';

export type UpdateAllFailure = {
  step: UpdateAllStep;
  error: unknown;
  /** The store already published this file-safety failure to the user. */
  reportedByStore?: true;
};

export type UpdateAllOwner = Readonly<{
  gameId: string;
  installPath: string;
}>;

/** Signals that the captured batch no longer belongs to the selected game install. */
export class UpdateAllOwnerChangedError extends Error {
  constructor() {
    super('The selected game installation changed during Update All');
    this.name = 'UpdateAllOwnerChangedError';
  }
}

export type RunUpdateAllOptions = {
  items: readonly SwapRequest[];
  owner: UpdateAllOwner;
  isOwnerCurrent: () => boolean;
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
  owner,
  isOwnerCurrent,
  addonUpdates,
  onBulkSwap,
}: RunUpdateAllOptions): Promise<void> {
  const failures: UpdateAllFailure[] = [];

  function assertOwnerCurrent(): void {
    if (!isOwnerCurrent()) {
      throw new UpdateAllOwnerChangedError();
    }
  }

  if (items.length > 0) {
    assertOwnerCurrent();
    try {
      await onBulkSwap(items);
      assertOwnerCurrent();
    } catch (error) {
      if (!isOwnerCurrent()) {
        throw new UpdateAllOwnerChangedError();
      }
      failures.push({ step: 'libraries', error });
      if (isFileSafetyContextError(error)) {
        throw new UpdateAllError(failures);
      }
    }
  }

  if (owner.gameId) {
    for (const { step, store } of addonUpdates) {
      assertOwnerCurrent();
      try {
        // `update` returns a tri-state; only hard failures are aggregated.
        // Errors are usually notified inside the store and not rethrown.
        const result = await store.update(owner.gameId);
        assertOwnerCurrent();
        if (!isMutationFailure(result)) {
          continue;
        }
        if (store.safetyContextError) {
          failures.push({ step, error: store.safetyContextError, reportedByStore: true });
          break;
        }
        const error = new ClientError('update_all_step_failed', { step });
        failures.push({ step, error });
      } catch (error) {
        if (!isOwnerCurrent()) {
          throw new UpdateAllOwnerChangedError();
        }
        if (store.safetyContextError && isFileSafetyContextError(store.safetyContextError)) {
          failures.push({
            step,
            error: store.safetyContextError,
            reportedByStore: true,
          });
          break;
        }
        failures.push({ step, error });
        if (isFileSafetyContextError(error)) {
          break;
        }
      }
    }
  }

  if (failures.length > 0) {
    throw new UpdateAllError(failures);
  }
}
