import { formatPresentedError, presentError } from '@shared/error-presentation';
import { t } from '@shared/i18n';
import type {
  GameRemovalAssessment,
  NvapiClaimAbandonmentAuthorization,
  NvapiClaimAbandonmentToken,
  NvapiClaimAssessment,
} from './types';

export type RemoveGameFlowDeps = {
  assess: (gameId: string) => Promise<GameRemovalAssessment>;
  remove: (gameId: string, authorization?: NvapiClaimAbandonmentAuthorization) => Promise<boolean>;
};

export type RemoveGameFlow = ReturnType<typeof createRemoveGameFlow>;

export type ValidatedRemovalPlan =
  | { kind: 'automatic' }
  | { kind: 'requires_authorization'; claims: NvapiClaimAssessment[] }
  | { kind: 'blocked'; reason: string };

/** Matches machine-readable error codes for stale driver authorization. */
export function isStaleAuthorizationError(error: unknown): boolean {
  const presented = presentError(error);
  return (
    presented.code === 'game_removal_cleanup_failed' &&
    presented.reasonCode === 'stale_authorization'
  );
}

/** Matches errors indicating blocked release (witness mismatch or uncertain live state). */
export function isBlockedRemovalError(error: unknown): boolean {
  const presented = presentError(error);
  return (
    presented.code === 'game_removal_cleanup_failed' &&
    (presented.reasonCode === 'witness_mismatch' ||
      presented.reasonCode === 'uncertain_observation')
  );
}

/** Validates preflight removal assessment against safety and consistency invariants. */
export function validateRemovalAssessment(
  requestedGameId: string,
  assessment: GameRemovalAssessment,
): ValidatedRemovalPlan {
  const normalizedId = requestedGameId.trim();
  if (!normalizedId) {
    return { kind: 'blocked', reason: 'Game identifier is required.' };
  }

  if (assessment.gameId !== normalizedId) {
    return {
      kind: 'blocked',
      reason: `Assessment identity mismatch: expected ${normalizedId}, received ${assessment.gameId}.`,
    };
  }

  const { canRemoveAutomatically, nvapiAssessment } = assessment;

  if (nvapiAssessment.blockedReason !== null) {
    return { kind: 'blocked', reason: nvapiAssessment.blockedReason };
  }

  const blockedClaim = nvapiAssessment.claims.find((c) => c.disposition === 'blocked');
  if (blockedClaim) {
    return {
      kind: 'blocked',
      reason: `Setting ${blockedClaim.settingName || blockedClaim.settingId} in ${blockedClaim.targetId} is blocked from release.`,
    };
  }

  if (canRemoveAutomatically && nvapiAssessment.requiresUserAuthorization) {
    return {
      kind: 'blocked',
      reason: 'Inconsistent assessment: conflicting automatic and authorized removal flags.',
    };
  }

  if (!canRemoveAutomatically && !nvapiAssessment.requiresUserAuthorization) {
    return {
      kind: 'blocked',
      reason: 'This game cannot be removed automatically from the catalog.',
    };
  }

  const authClaims = nvapiAssessment.claims.filter(
    (c) => c.disposition === 'requiresAbandonmentAuthorization',
  );

  if (canRemoveAutomatically) {
    if (authClaims.length > 0) {
      return {
        kind: 'blocked',
        reason: 'Inconsistent assessment: claims require authorization during automatic removal.',
      };
    }
    return { kind: 'automatic' };
  }

  if (authClaims.length === 0) {
    return {
      kind: 'blocked',
      reason: 'Inconsistent assessment: authorization required but no eligible claims present.',
    };
  }

  // Live driver state must be verified to prevent releasing uninspected driver settings
  const uncertainClaim = authClaims.find((c) => c.observedLive === null);
  if (uncertainClaim) {
    return {
      kind: 'blocked',
      reason: 'Uncertain driver observation: live setting state could not be verified.',
    };
  }

  return { kind: 'requires_authorization', claims: authClaims };
}

/** Builds verified claim-abandonment authorization tokens from assessment. */
export function buildAbandonmentAuthorization(
  gameId: string,
  assessment: GameRemovalAssessment,
): NvapiClaimAbandonmentAuthorization {
  const normalizedGameId = gameId.trim();
  if (!normalizedGameId) {
    throw new Error('Game ID is required to build abandonment authorization.');
  }

  const plan = validateRemovalAssessment(normalizedGameId, assessment);
  if (plan.kind !== 'requires_authorization') {
    throw new Error(
      plan.kind === 'blocked'
        ? `Cannot build abandonment authorization: ${plan.reason}`
        : 'Assessment does not require abandonment authorization.',
    );
  }

  const tokens: NvapiClaimAbandonmentToken[] = plan.claims.map((c) => {
    if (!c.observedLive) {
      throw new Error(
        `Cannot build token for setting ${c.settingName || c.settingId}: observedLive is missing.`,
      );
    }
    return {
      targetId: c.targetId,
      settingId: c.settingId,
      expectedPresent: c.recordedExpected.present,
      expectedValue: c.recordedExpected.value,
      originalPresent: c.recordedOriginal.present,
      originalValue: c.recordedOriginal.value,
      observedLivePresent: c.observedLive.present,
      observedLiveValue: c.observedLive.value,
    };
  });

  return {
    gameId: normalizedGameId,
    claims: tokens,
  };
}

export function createRemoveGameFlow(deps: RemoveGameFlowDeps) {
  let open = $state(false);
  let gameId = $state<string | null>(null);
  let gameTitle = $state('');
  let assessing = $state(false);
  let submitting = $state(false);
  let assessment = $state<GameRemovalAssessment | null>(null);
  let error = $state<string | null>(null);
  let recoveryBundlePath = $state<string | null>(null);
  let runtimeBlockedReason = $state<string | null>(null);
  let staleWarning = $state(false);
  let currentSequence = 0;

  const removalPlan = $derived.by((): ValidatedRemovalPlan | null => {
    if (!open || assessing || !gameId || !assessment) {
      return null;
    }
    return validateRemovalAssessment(gameId, assessment);
  });

  const blockedReason = $derived(
    runtimeBlockedReason ?? (removalPlan?.kind === 'blocked' ? removalPlan.reason : null),
  );

  const divergedClaims = $derived.by((): NvapiClaimAssessment[] => {
    return removalPlan?.kind === 'requires_authorization' ? removalPlan.claims : [];
  });

  const isDiverged = $derived(divergedClaims.length > 0);

  const canConfirm = $derived(
    !submitting && !assessing && removalPlan !== null && blockedReason === null && error === null,
  );

  async function runAssessment(targetGameId: string): Promise<boolean> {
    const seq = ++currentSequence;
    assessing = true;
    error = null;
    recoveryBundlePath = null;
    runtimeBlockedReason = null;

    try {
      const result = await deps.assess(targetGameId);
      if (seq !== currentSequence || !open) {
        return false;
      }

      assessment = result;
      const plan = validateRemovalAssessment(targetGameId, result);
      return plan.kind !== 'blocked';
    } catch (assessError) {
      if (seq !== currentSequence || !open) {
        return false;
      }
      assessment = null;
      const presented = presentError(assessError);
      error = formatPresentedError(assessError);
      recoveryBundlePath = presented.recoveryBundlePath ?? null;
      if (isBlockedRemovalError(assessError)) {
        runtimeBlockedReason = error;
      }
      return false;
    } finally {
      if (seq === currentSequence) {
        assessing = false;
      }
    }
  }

  async function openConfirmation(targetId: string, title: string): Promise<void> {
    if (submitting) {
      return;
    }
    const cleanId = targetId.trim();
    open = true;
    gameId = cleanId || null;
    gameTitle = title;
    assessment = null;
    error = null;
    recoveryBundlePath = null;
    runtimeBlockedReason = null;
    staleWarning = false;

    if (!cleanId) {
      runtimeBlockedReason = 'Game identifier is required.';
      return;
    }

    await runAssessment(cleanId);
  }

  function close(force = false): void {
    if (submitting && !force) {
      return;
    }
    open = false;
    currentSequence++;
    assessing = false;
    submitting = false;
    assessment = null;
    error = null;
    recoveryBundlePath = null;
    runtimeBlockedReason = null;
    staleWarning = false;
    gameId = null;
    gameTitle = '';
  }

  function cancel(): void {
    close();
  }

  function setOpen(nextOpen: boolean): void {
    if (!nextOpen) {
      close();
    }
  }

  async function retryAssessment(): Promise<void> {
    if (gameId && !submitting && !assessing) {
      await runAssessment(gameId);
    }
  }

  async function confirm(): Promise<boolean> {
    if (!canConfirm || !gameId || !assessment || !removalPlan || removalPlan.kind === 'blocked') {
      return false;
    }

    let authorization: NvapiClaimAbandonmentAuthorization | undefined;
    if (removalPlan.kind === 'requires_authorization') {
      try {
        authorization = buildAbandonmentAuthorization(gameId, assessment);
      } catch (authError) {
        runtimeBlockedReason = formatPresentedError(authError);
        return false;
      }
    }

    submitting = true;
    error = null;
    recoveryBundlePath = null;
    staleWarning = false;

    try {
      const removed = await deps.remove(gameId, authorization);
      if (removed) {
        close(true);
        return true;
      }
      assessment = null;
      error = t('notify.removeGameFailed');
      return false;
    } catch (removeError) {
      const presented = presentError(removeError);
      const formattedError = formatPresentedError(removeError);
      recoveryBundlePath = presented.recoveryBundlePath ?? null;

      if (isStaleAuthorizationError(removeError)) {
        // Re-assess live state so user can review changed driver settings.
        // Never auto-retry removal: user must explicitly re-confirm new divergent claims.
        const reassessedOk = await runAssessment(gameId);
        if (reassessedOk) {
          staleWarning = true;
        }
      } else if (isBlockedRemovalError(removeError)) {
        runtimeBlockedReason = formattedError;
        error = null;
        assessment = null;
      } else {
        // Invalidate stale preflight assessment after removal failure so retry requires fresh assessment.
        assessment = null;
        error = formattedError;
      }
      return false;
    } finally {
      submitting = false;
    }
  }

  return {
    get open() {
      return open;
    },
    get gameId() {
      return gameId;
    },
    get gameTitle() {
      return gameTitle;
    },
    get assessing() {
      return assessing;
    },
    get submitting() {
      return submitting;
    },
    get assessment() {
      return assessment;
    },
    get error() {
      return error;
    },
    get recoveryBundlePath() {
      return recoveryBundlePath;
    },
    get blockedReason() {
      return blockedReason;
    },
    get divergedClaims() {
      return divergedClaims;
    },
    get isDiverged() {
      return isDiverged;
    },
    get staleWarning() {
      return staleWarning;
    },
    get canConfirm() {
      return canConfirm;
    },
    openConfirmation,
    setOpen,
    retryAssessment,
    confirm,
    cancel,
    close: () => {
      close();
    },
  };
}
