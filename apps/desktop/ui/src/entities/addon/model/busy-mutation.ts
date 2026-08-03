import { reportClientError } from '@shared/errors';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { clearDownloadProgress } from '@shared/lib';

import {
  withBusy,
  withMutationBegin,
  withProbeBegin,
  type AddonCoreSnapshot,
  type AddonInstallStateBase,
  type FreshnessSource,
} from './store-helpers';

/** Why a probe is running -- tools map this to wire options. */
export type CheckUpdateKind = 'user' | 'passive';

/**
 * Default upstream re-probe after a successful mutation commit.
 * - `never` -- keep the synthetic install report (RenoDX).
 * - `passive` -- re-probe without deep ZIP downloads (Luma dgVoodoo / overall).
 * Per-call `runBusyMutation({ probeUpdates })` still overrides.
 */
export type PostMutationProbe = 'never' | 'passive';

/**
 * Outcome of a store mutation (`install` / `update` / `uninstall`, ...).
 * Soft no-ops (busy, not eligible) are `skipped` so bulk workflows do not
 * treat them as user-visible failures.
 */
export type AddonMutationResult = 'ok' | 'skipped' | 'failed';

export function isMutationFailure(result: AddonMutationResult): boolean {
  return result === 'failed';
}

export function isMutationSuccess(result: AddonMutationResult): boolean {
  return result === 'ok';
}

export type BusyMutationOptions = {
  errorKey: MessageKeyWithoutParams;
  clearDownloadProgress?: boolean;
  requireUpdateAvailable?: boolean;
  afterCommit?: (token: number) => void | Promise<void>;
  notifyExclusivity?: boolean;
  /**
   * Re-probe upstream update status with the commit token (no new requestId).
   * Prefer this over `checkForUpdates` inside `afterCommit`, which would bump
   * requestId and skip exclusivity notify / busy clear.
   * When omitted, uses store `postMutationProbe` (`passive` -> true, else false).
   */
  probeUpdates?: boolean;
};

/**
 * Explicit port into the reactive store core so mutation orchestration does not
 * close over `$state` directly.
 */
export type BusyMutationContext<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
> = {
  getCore: () => AddonCoreSnapshot<TState, TUpdateReport>;
  setCore: (next: AddonCoreSnapshot<TState, TUpdateReport>) => void;
  getUpdateAvailable: () => boolean;
  commitMutationResult: (nextState: TState) => number;
  refreshHostInfo: (gameId: string, token: number) => Promise<void>;
  probeUpdateStatus: (gameId: string, token: number, kind: CheckUpdateKind) => Promise<void>;
  notifyExclusivityChange: (gameId: string) => void;
  postMutationProbe: PostMutationProbe;
  onMutationSideEffect?: (gameId: string, token: number) => void | Promise<void>;
};

/** Pure gate: busy or not eligible for an update-gated mutation. */
export function shouldSkipBusyMutation(
  busy: boolean,
  requireUpdateAvailable: boolean | undefined,
  updateAvailable: boolean,
): boolean {
  if (busy) {
    return true;
  }
  if (requireUpdateAvailable && !updateAvailable) {
    return true;
  }
  return false;
}

export function resolveShouldProbe(
  probeUpdates: boolean | undefined,
  postMutationProbe: PostMutationProbe,
): boolean {
  return probeUpdates ?? postMutationProbe === 'passive';
}

export async function runBusyMutation<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
>(
  ctx: BusyMutationContext<TState, TUpdateReport>,
  gameId: string,
  fn: () => Promise<TState>,
  options: BusyMutationOptions,
): Promise<AddonMutationResult> {
  const core = ctx.getCore();
  if (shouldSkipBusyMutation(core.busy, options.requireUpdateAvailable, ctx.getUpdateAvailable())) {
    return 'skipped';
  }

  const { next, token: mutationToken } = withMutationBegin(core);
  ctx.setCore(next);
  // Tracks which requestId still owns `busy`. Commit bumps requestId, so the
  // owner becomes the commit token; a superseding load leaves a stale owner
  // and already cleared busy via withLoadBegin.
  let busyOwnerToken = mutationToken;
  if (options.clearDownloadProgress !== false) {
    clearDownloadProgress([gameId]);
  }

  try {
    let nextState: TState;
    try {
      nextState = await fn();
    } catch (error) {
      if (mutationToken === ctx.getCore().requestId) {
        publishPresentedErrorNotification(t(options.errorKey), error);
      }
      return 'failed';
    }

    // Load (or another request) superseded this mutation -- discard the paint
    // so game A cannot land on the store after the user navigated to game B.
    // Backend already committed: still notify peers so exclusivity badges update.
    // Report `ok` so bulk update-all does not treat a successful backend
    // commit as a user-visible failure when paint was discarded.
    if (mutationToken !== ctx.getCore().requestId) {
      if (options.notifyExclusivity) {
        try {
          ctx.notifyExclusivityChange(gameId);
        } catch (error) {
          reportClientError('addon_exclusivity_refresh', error, 'warning');
        }
      }
      return 'ok';
    }

    const token = ctx.commitMutationResult(nextState);
    busyOwnerToken = token;
    const shouldProbe = resolveShouldProbe(options.probeUpdates, ctx.postMutationProbe);
    // Mark freshness as `checking` but keep the synthetic install report so a
    // passive "unknown" can coalesce back to "current" via coalesceUpdateReport
    // instead of flashing a false "couldn't check" right after install.
    if (shouldProbe) {
      ctx.setCore(withProbeBegin(ctx.getCore()));
    }
    await ctx.refreshHostInfo(gameId, token);
    if (shouldProbe) {
      await ctx.probeUpdateStatus(gameId, token, 'passive');
    }
    try {
      await options.afterCommit?.(token);
    } catch (error) {
      reportClientError('addon_post_commit_refresh', error, 'warning');
    }
    if (token === ctx.getCore().requestId && ctx.onMutationSideEffect) {
      try {
        await ctx.onMutationSideEffect(gameId, token);
      } catch (error) {
        reportClientError('addon_post_mutation_side_effect', error, 'warning');
      }
    }
    if (options.notifyExclusivity && token === ctx.getCore().requestId) {
      try {
        ctx.notifyExclusivityChange(gameId);
      } catch (error) {
        reportClientError('addon_exclusivity_refresh', error, 'warning');
      }
    }
    // Backend mutation already committed; post-commit hooks are best-effort.
    return 'ok';
  } finally {
    if (busyOwnerToken === ctx.getCore().requestId) {
      ctx.setCore(withBusy(ctx.getCore(), false));
    }
  }
}
