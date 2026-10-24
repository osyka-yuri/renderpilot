import { describe, expect, it, vi } from 'vitest';
import { DesktopCommandError } from '@shared/errors';
import { t } from '@shared/i18n';
import type {
  GameRemovalAssessment,
  NvapiClaimAbandonmentAuthorization,
  NvapiClaimAssessment,
} from './types';
import {
  buildAbandonmentAuthorization,
  createRemoveGameFlow,
  isBlockedRemovalError,
  isStaleAuthorizationError,
  validateRemovalAssessment,
} from './remove-game-flow.svelte';

function makeClaim(overrides: Partial<NvapiClaimAssessment> = {}): NvapiClaimAssessment {
  return {
    targetId: 'Base Profile',
    settingId: 0x10b3292c,
    settingName: 'DLSS Preset',
    status: 'diverged',
    recordedExpected: { present: true, value: 1 },
    recordedOriginal: { present: false, value: null },
    observedLive: { present: true, value: 2 },
    disposition: 'requiresAbandonmentAuthorization',
    ...overrides,
  };
}

function makeAssessment(overrides: Partial<GameRemovalAssessment> = {}): GameRemovalAssessment {
  return {
    gameId: 'game-1',
    canRemoveAutomatically: true,
    nvapiAssessment: {
      requiresUserAuthorization: false,
      blockedReason: null,
      claims: [],
    },
    ...overrides,
  };
}

describe('remove-game-flow model', () => {
  it('normal removal runs preflight assessment and confirms cleanly without authorization', async () => {
    const assess = vi.fn(() => Promise.resolve(makeAssessment()));
    const remove = vi.fn((_gameId: string, _auth?: NvapiClaimAbandonmentAuthorization) =>
      Promise.resolve(true),
    );

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(assess).toHaveBeenCalledWith('game-1');
    expect(flow.open).toBe(true);
    expect(flow.gameTitle).toBe('Cyberpunk 2077');
    expect(flow.canConfirm).toBe(true);
    expect(flow.isDiverged).toBe(false);

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(true);
    expect(remove).toHaveBeenCalledWith('game-1', undefined);
    expect(flow.open).toBe(false);
  });

  it('divergent claim detects required authorization and builds token upon confirmation', async () => {
    const assessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim()],
      },
    });

    const assess = vi.fn(() => Promise.resolve(assessment));
    let capturedAuth: NvapiClaimAbandonmentAuthorization | undefined;
    const remove = vi.fn((_gameId: string, auth?: NvapiClaimAbandonmentAuthorization) => {
      capturedAuth = auth;
      return Promise.resolve(true);
    });

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.isDiverged).toBe(true);
    expect(flow.divergedClaims).toHaveLength(1);
    expect(flow.canConfirm).toBe(true);

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(true);
    expect(capturedAuth).toEqual({
      gameId: 'game-1',
      claims: [
        {
          targetId: 'Base Profile',
          settingId: 0x10b3292c,
          expectedPresent: true,
          expectedValue: 1,
          originalPresent: false,
          originalValue: null,
          observedLivePresent: true,
          observedLiveValue: 2,
        },
      ],
    });
  });

  it('preflight assessment failure blocks removal and allows retry', async () => {
    let callCount = 0;
    const assess = vi.fn(() => {
      callCount++;
      if (callCount === 1) {
        return Promise.reject(new Error('IPC network error'));
      }
      return Promise.resolve(makeAssessment());
    });
    const remove = vi.fn(() => Promise.resolve(true));

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.assessment).toBeNull();
    expect(flow.error).toBe(t('error.unexpectedClient'));
    expect(flow.canConfirm).toBe(false);

    const confirmedEarly = await flow.confirm();
    expect(confirmedEarly).toBe(false);
    expect(remove).not.toHaveBeenCalled();

    await flow.retryAssessment();
    expect(flow.assessment).not.toBeNull();
    expect(flow.error).toBeNull();
    expect(flow.canConfirm).toBe(true);

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(true);
    expect(remove).toHaveBeenCalledWith('game-1', undefined);
  });

  it('inconsistent assessment flags fail closed and block removal', async () => {
    // Both false
    const neitherAllowed = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: false,
        blockedReason: null,
        claims: [],
      },
    });

    const assessNeither = vi.fn(() => Promise.resolve(neitherAllowed));
    const flowNeither = createRemoveGameFlow({ assess: assessNeither, remove: vi.fn() });
    await flowNeither.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flowNeither.blockedReason).toContain('cannot be removed automatically');
    expect(flowNeither.canConfirm).toBe(false);

    // Both true (contradiction)
    const bothAllowed = makeAssessment({
      canRemoveAutomatically: true,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim()],
      },
    });
    const assessBoth = vi.fn(() => Promise.resolve(bothAllowed));
    const flowBoth = createRemoveGameFlow({ assess: assessBoth, remove: vi.fn() });
    await flowBoth.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flowBoth.blockedReason).toContain('conflicting automatic and authorized removal flags');
    expect(flowBoth.canConfirm).toBe(false);

    // Automatic mode with authorization claims
    const automaticWithAuthClaims = makeAssessment({
      canRemoveAutomatically: true,
      nvapiAssessment: {
        requiresUserAuthorization: false,
        blockedReason: null,
        claims: [makeClaim()],
      },
    });
    const assessInconsistent = vi.fn(() => Promise.resolve(automaticWithAuthClaims));
    const flowInconsistent = createRemoveGameFlow({ assess: assessInconsistent, remove: vi.fn() });
    await flowInconsistent.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flowInconsistent.blockedReason).toContain(
      'require authorization during automatic removal',
    );
    expect(flowInconsistent.canConfirm).toBe(false);

    // Authorization mode with 0 authorization claims
    const authModeNoClaims = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [],
      },
    });
    const assessNoClaims = vi.fn(() => Promise.resolve(authModeNoClaims));
    const flowNoClaims = createRemoveGameFlow({ assess: assessNoClaims, remove: vi.fn() });
    await flowNoClaims.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flowNoClaims.blockedReason).toContain('no eligible claims present');
    expect(flowNoClaims.canConfirm).toBe(false);
  });

  it('blocked disposition on claim blocks removal', async () => {
    const blockedClaimAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: false,
        blockedReason: null,
        claims: [makeClaim({ status: 'blocked', disposition: 'blocked' })],
      },
    });

    const assess = vi.fn(() => Promise.resolve(blockedClaimAssessment));
    const flow = createRemoveGameFlow({ assess, remove: vi.fn() });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.blockedReason).toContain('is blocked from release');
    expect(flow.canConfirm).toBe(false);
  });

  it('assessment gameId identity mismatch blocks removal and token building', async () => {
    const wrongGameAssessment = makeAssessment({
      gameId: 'game-other',
      canRemoveAutomatically: true,
    });

    const assess = vi.fn(() => Promise.resolve(wrongGameAssessment));
    const flow = createRemoveGameFlow({ assess, remove: vi.fn() });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.blockedReason).toContain('Assessment identity mismatch');
    expect(flow.canConfirm).toBe(false);

    expect(() => buildAbandonmentAuthorization('game-1', wrongGameAssessment)).toThrow(
      /Assessment identity mismatch/,
    );
  });

  it('observedLive: null in divergent claim fails closed with uncertain observation', async () => {
    const uncertainAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim({ status: 'uncertain', observedLive: null })],
      },
    });

    const assess = vi.fn(() => Promise.resolve(uncertainAssessment));
    const remove = vi.fn(() => Promise.resolve(true));

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.blockedReason).toContain('Uncertain driver observation');
    expect(flow.canConfirm).toBe(false);

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(false);
    expect(remove).not.toHaveBeenCalled();

    expect(() => buildAbandonmentAuthorization('game-1', uncertainAssessment)).toThrow(
      /Cannot build abandonment authorization/,
    );
  });

  it('stale async responses are discarded when dialog closes or restarts', async () => {
    let resolveFirstAssessment!: (value: GameRemovalAssessment) => void;
    const firstPromise = new Promise<GameRemovalAssessment>((res) => {
      resolveFirstAssessment = res;
    });

    const assess = vi.fn().mockImplementationOnce(() => firstPromise);
    const remove = vi.fn(() => Promise.resolve(true));

    const flow = createRemoveGameFlow({ assess, remove });
    void flow.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flow.assessing).toBe(true);

    flow.close();
    expect(flow.open).toBe(false);

    resolveFirstAssessment(makeAssessment());
    await Promise.resolve();

    expect(flow.open).toBe(false);
    expect(flow.assessment).toBeNull();
    expect(flow.assessing).toBe(false);
  });

  it('concurrent confirm calls while submitting are rejected', async () => {
    let resolveRemove!: (value: boolean) => void;
    const removePromise = new Promise<boolean>((res) => {
      resolveRemove = res;
    });

    const assess = vi.fn(() => Promise.resolve(makeAssessment()));
    const remove = vi.fn(() => removePromise);

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    const firstConfirmPromise = flow.confirm();
    expect(flow.submitting).toBe(true);

    const secondConfirmPromise = flow.confirm();
    expect(await secondConfirmPromise).toBe(false);
    expect(remove).toHaveBeenCalledTimes(1);

    resolveRemove(true);
    expect(await firstConfirmPromise).toBe(true);
  });

  it('stale authorization triggers re-assessment without auto-retrying removal', async () => {
    const initialAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim({ observedLive: { present: true, value: 2 } })],
      },
    });

    const freshAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim({ observedLive: { present: true, value: 3 } })],
      },
    });

    const assess = vi
      .fn()
      .mockResolvedValueOnce(initialAssessment)
      .mockResolvedValueOnce(freshAssessment);

    let capturedAuth: NvapiClaimAbandonmentAuthorization | undefined;
    const remove = vi
      .fn()
      .mockRejectedValueOnce(
        DesktopCommandError.fromDto({
          code: 'game_removal_cleanup_failed',
          reasonCode: 'stale_authorization',
        }),
      )
      .mockImplementationOnce((_id: string, auth?: NvapiClaimAbandonmentAuthorization) => {
        capturedAuth = auth;
        return Promise.resolve(true);
      });

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    const firstConfirmed = await flow.confirm();
    expect(firstConfirmed).toBe(false);

    expect(assess).toHaveBeenCalledTimes(2);
    expect(flow.staleWarning).toBe(true);
    expect(flow.open).toBe(true);
    expect(flow.submitting).toBe(false);
    expect(flow.divergedClaims[0].observedLive?.value).toBe(3);
    expect(remove).toHaveBeenCalledTimes(1);

    const secondConfirmed = await flow.confirm();
    expect(secondConfirmed).toBe(true);
    expect(remove).toHaveBeenCalledTimes(2);
    expect(capturedAuth?.claims[0].observedLiveValue).toBe(3);
  });

  it('reassessment failure after stale authorization preserves error and does not hide it behind staleWarning', async () => {
    const initialAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [makeClaim()],
      },
    });

    const assess = vi
      .fn()
      .mockResolvedValueOnce(initialAssessment)
      .mockRejectedValueOnce(new Error('Driver communication timeout during reassessment'))
      .mockResolvedValueOnce(initialAssessment);

    const remove = vi.fn().mockRejectedValueOnce(
      DesktopCommandError.fromDto({
        code: 'game_removal_cleanup_failed',
        reasonCode: 'stale_authorization',
      }),
    );

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(false);

    expect(flow.staleWarning).toBe(false);
    expect(flow.error).toBe(t('error.unexpectedClient'));
    expect(flow.canConfirm).toBe(false);
    expect(flow.assessment).toBeNull();

    await flow.retryAssessment();
    expect(flow.error).toBeNull();
    expect(flow.assessment).not.toBeNull();
    expect(flow.canConfirm).toBe(true);
  });

  it('blocked removal error sets blockedReason and stops removal', async () => {
    const assess = vi.fn(() => Promise.resolve(makeAssessment()));
    const remove = vi.fn(() =>
      Promise.reject(
        DesktopCommandError.fromDto({
          code: 'game_removal_cleanup_failed',
          reasonCode: 'witness_mismatch',
        }),
      ),
    );

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    const confirmed = await flow.confirm();
    expect(confirmed).toBe(false);
    expect(flow.blockedReason).not.toBeNull();
    expect(flow.canConfirm).toBe(false);
  });

  it('removal failure invalidates previous assessment and recovers via retry assessment', async () => {
    const assess = vi.fn(() => Promise.resolve(makeAssessment()));
    const remove = vi
      .fn()
      .mockRejectedValueOnce(new Error('Filesystem lock prevented file removal'))
      .mockResolvedValueOnce(true);

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flow.canConfirm).toBe(true);

    const firstConfirm = await flow.confirm();
    expect(firstConfirm).toBe(false);
    expect(flow.error).toBe(t('error.unexpectedClient'));
    expect(flow.assessment).toBeNull();
    expect(flow.canConfirm).toBe(false);

    await flow.retryAssessment();
    expect(assess).toHaveBeenCalledTimes(2);
    expect(flow.error).toBeNull();
    expect(flow.assessment).not.toBeNull();
    expect(flow.canConfirm).toBe(true);

    const secondConfirm = await flow.confirm();
    expect(secondConfirm).toBe(true);
    expect(remove).toHaveBeenCalledTimes(2);
    expect(flow.open).toBe(false);
  });

  it('backend false removal result invalidates assessment and allows recovery via retry assessment', async () => {
    const assess = vi.fn(() => Promise.resolve(makeAssessment()));
    const remove = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');
    expect(flow.canConfirm).toBe(true);

    const firstConfirm = await flow.confirm();
    expect(firstConfirm).toBe(false);
    expect(flow.error).toBe(t('notify.removeGameFailed'));
    expect(flow.assessment).toBeNull();
    expect(flow.canConfirm).toBe(false);

    await flow.retryAssessment();
    expect(flow.error).toBeNull();
    expect(flow.canConfirm).toBe(true);

    const secondConfirm = await flow.confirm();
    expect(secondConfirm).toBe(true);
  });

  it('multiple divergent claims are represented and authorized correctly', async () => {
    const multiAssessment = makeAssessment({
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [
          makeClaim({ settingId: 0x10b3292c, settingName: 'DLSS Preset' }),
          makeClaim({
            targetId: 'Game Profile',
            settingId: 0x10b3292d,
            settingName: 'DLSS FG',
            recordedExpected: { present: true, value: 0 },
            recordedOriginal: { present: true, value: 1 },
            observedLive: { present: true, value: 1 },
          }),
        ],
      },
    });

    const assess = vi.fn(() => Promise.resolve(multiAssessment));
    let capturedAuth: NvapiClaimAbandonmentAuthorization | undefined;
    const remove = vi.fn((_id: string, auth?: NvapiClaimAbandonmentAuthorization) => {
      capturedAuth = auth;
      return Promise.resolve(true);
    });

    const flow = createRemoveGameFlow({ assess, remove });
    await flow.openConfirmation('game-1', 'Cyberpunk 2077');

    expect(flow.divergedClaims).toHaveLength(2);
    await flow.confirm();

    expect(capturedAuth?.claims).toHaveLength(2);
    expect(capturedAuth?.claims[0].settingId).toBe(0x10b3292c);
    expect(capturedAuth?.claims[1].settingId).toBe(0x10b3292d);
  });

  it('helper error classifiers accurately classify errors', () => {
    expect(
      isStaleAuthorizationError(
        DesktopCommandError.fromDto({
          code: 'game_removal_cleanup_failed',
          reasonCode: 'stale_authorization',
        }),
      ),
    ).toBe(true);

    expect(
      isBlockedRemovalError(
        DesktopCommandError.fromDto({
          code: 'game_removal_cleanup_failed',
          reasonCode: 'witness_mismatch',
        }),
      ),
    ).toBe(true);

    expect(
      isBlockedRemovalError(
        DesktopCommandError.fromDto({
          code: 'game_removal_cleanup_failed',
          reasonCode: 'uncertain_observation',
        }),
      ),
    ).toBe(true);

    expect(isStaleAuthorizationError(new Error('unrelated error'))).toBe(false);
    expect(isBlockedRemovalError(new Error('unrelated error'))).toBe(false);
  });

  it('validateRemovalAssessment directly enforces fail-closed and returns appropriate plan', () => {
    expect(validateRemovalAssessment('game-1', makeAssessment({ gameId: 'other-game' }))).toEqual({
      kind: 'blocked',
      reason: 'Assessment identity mismatch: expected game-1, received other-game.',
    });

    expect(validateRemovalAssessment('  ', makeAssessment())).toEqual({
      kind: 'blocked',
      reason: 'Game identifier is required.',
    });

    expect(validateRemovalAssessment('game-1', makeAssessment())).toEqual({
      kind: 'automatic',
    });

    const authClaim = makeClaim();
    const validAuthPlan = validateRemovalAssessment(
      'game-1',
      makeAssessment({
        canRemoveAutomatically: false,
        nvapiAssessment: {
          requiresUserAuthorization: true,
          blockedReason: null,
          claims: [authClaim],
        },
      }),
    );
    expect(validAuthPlan).toEqual({
      kind: 'requires_authorization',
      claims: [authClaim],
    });

    expect(
      validateRemovalAssessment(
        'game-1',
        makeAssessment({
          canRemoveAutomatically: true,
          nvapiAssessment: {
            requiresUserAuthorization: true,
            blockedReason: null,
            claims: [authClaim],
          },
        }),
      ),
    ).toEqual({
      kind: 'blocked',
      reason: 'Inconsistent assessment: conflicting automatic and authorized removal flags.',
    });

    expect(
      validateRemovalAssessment(
        'game-1',
        makeAssessment({
          canRemoveAutomatically: false,
          nvapiAssessment: {
            requiresUserAuthorization: false,
            blockedReason: null,
            claims: [],
          },
        }),
      ),
    ).toEqual({
      kind: 'blocked',
      reason: 'This game cannot be removed automatically from the catalog.',
    });

    expect(
      validateRemovalAssessment(
        'game-1',
        makeAssessment({
          canRemoveAutomatically: false,
          nvapiAssessment: {
            requiresUserAuthorization: true,
            blockedReason: null,
            claims: [makeClaim({ observedLive: null })],
          },
        }),
      ),
    ).toEqual({
      kind: 'blocked',
      reason: 'Uncertain driver observation: live setting state could not be verified.',
    });
  });
});
