import { describe, expect, it, vi } from 'vitest';
import type { PresentedError } from '@shared/error-presentation';
import { createCleanupFlow, type CleanupFlowDeps } from './cleanup-flow.svelte';
import type {
  CleanupOutcome,
  LeftoverProposal,
  LeaveOutcome,
  ListRetiredGameLeftoversOutput,
} from './types';

function proposal(gameId = 'game:a', intent = 'intent:a'): LeftoverProposal {
  return {
    gameId,
    gameName: gameId,
    installPath: `D:/Games/${gameId}`,
    rootState: 'missing',
    intent,
    leaveRevision: 'revision:1',
    items: [
      {
        itemId: 'file:1',
        category: 'optiscalerFile',
        path: 'D:/Games/game/dxgi.dll',
        disposition: 'cleanable',
        issue: null,
      },
    ],
    canClean: true,
  };
}

function completed(gameId: string): CleanupOutcome {
  return { gameId, status: 'complete', steps: [], remainingProposal: null, issue: null };
}

function presented(error: unknown): PresentedError {
  return {
    code: 'test_error',
    severity: 'error',
    message: String(error),
    suggestedActions: [],
    contractStatus: 'known',
  };
}

function deps(overrides: Partial<CleanupFlowDeps> = {}): CleanupFlowDeps {
  return {
    list: vi.fn(() => Promise.resolve({ proposals: [proposal()], issues: [] })),
    clean: vi.fn<CleanupFlowDeps['clean']>(({ gameId }) => Promise.resolve(completed(gameId))),
    leave: vi.fn<CleanupFlowDeps['leave']>(({ gameId }) =>
      Promise.resolve({
        gameId,
        status: 'left',
        leaveRevision: 'revision:1',
        remainingProposal: null,
        issue: null,
      }),
    ),
    presentError: presented,
    publishError: vi.fn(),
    publishIssue: vi.fn(),
    ...overrides,
  };
}

describe('retired-game cleanup flow', () => {
  it('coalesces concurrent discovery and ignores responses after disposal', async () => {
    const pending = Promise.withResolvers<ListRetiredGameLeftoversOutput>();
    const dependencies = deps({ list: vi.fn(() => pending.promise) });
    const flow = createCleanupFlow(dependencies);
    const first = flow.refresh();
    expect(flow.refresh()).toBe(first);
    await Promise.resolve();
    expect(dependencies.list).toHaveBeenCalledOnce();
    flow.dispose();
    pending.resolve({ proposals: [proposal()], issues: [] });
    await first;
    expect(flow.proposals).toEqual([]);
    await flow.cleanAll();
    expect(dependencies.clean).not.toHaveBeenCalled();
  });

  it('recovers from a synchronously thrown query and permits later discovery', async () => {
    const dependencies = deps({
      list: vi
        .fn<CleanupFlowDeps['list']>()
        .mockImplementationOnce(() => {
          throw new Error('query failed');
        })
        .mockResolvedValue({ proposals: [proposal()], issues: [] }),
    });
    const flow = createCleanupFlow(dependencies);
    await flow.refresh();
    expect(dependencies.publishError).toHaveBeenCalledOnce();
    await flow.refresh();
    expect(flow.proposals).toHaveLength(1);
  });

  it('blocks duplicate Clean/Leave and defers scan discovery until the action completes', async () => {
    const pending = Promise.withResolvers<CleanupOutcome>();
    const dependencies = deps({ clean: vi.fn(() => pending.promise) });
    const flow = createCleanupFlow(dependencies);
    await flow.refresh();
    const clean = flow.cleanAll();
    await flow.cleanAll();
    await flow.leaveAll();
    await flow.refresh();
    expect(dependencies.clean).toHaveBeenCalledOnce();
    expect(dependencies.leave).not.toHaveBeenCalled();
    expect(dependencies.list).toHaveBeenCalledOnce();
    pending.resolve(completed('game:a'));
    await clean;
    expect(dependencies.list).toHaveBeenCalledTimes(2);
    expect(flow.busy).toBe(false);
  });

  it('keeps a partial result and uses the refreshed intent on retry', async () => {
    const remaining = proposal('game:a', 'intent:retry');
    const publishIssue = vi.fn<CleanupFlowDeps['publishIssue']>();
    const partial: CleanupOutcome = {
      gameId: 'game:a',
      status: 'partial',
      remainingProposal: remaining,
      issue: null,
      steps: [{ itemId: 'removed', category: 'optiscalerFile', outcome: 'removed', issue: null }],
    };
    const clean = vi
      .fn<CleanupFlowDeps['clean']>()
      .mockResolvedValueOnce(partial)
      .mockResolvedValueOnce(completed('game:a'));
    const flow = createCleanupFlow(deps({ clean, publishIssue }));
    await flow.refresh();
    await flow.cleanAll();
    expect(flow.outcomes['game:a'].steps).toEqual(partial.steps);
    expect(flow.proposals[0].intent).toBe('intent:retry');
    expect(publishIssue).not.toHaveBeenCalled();
    await flow.cleanAll();
    expect(clean).toHaveBeenNthCalledWith(2, { gameId: 'game:a', intent: 'intent:retry' });
    expect(flow.proposals).toEqual([]);
  });

  it('continues with another game after transport failure and obtains a new intent', async () => {
    const list = vi
      .fn<CleanupFlowDeps['list']>()
      .mockResolvedValueOnce({
        proposals: [proposal(), proposal('game:b', 'intent:b')],
        issues: [],
      })
      .mockResolvedValueOnce({
        proposals: [proposal('game:a', 'intent:after-failure')],
        issues: [],
      });
    const clean = vi
      .fn<CleanupFlowDeps['clean']>()
      .mockRejectedValueOnce(new Error('connection lost'))
      .mockResolvedValueOnce(completed('game:b'));
    const flow = createCleanupFlow(deps({ list, clean }));
    await flow.refresh();
    await flow.cleanAll();
    expect(clean).toHaveBeenNthCalledWith(2, { gameId: 'game:b', intent: 'intent:b' });
    expect(flow.proposals[0].intent).toBe('intent:after-failure');
    expect(flow.errors['game:a'].message).toContain('connection lost');
  });

  it('does not cancel backend cleanup on disposal or publish its late response', async () => {
    const pending = Promise.withResolvers<CleanupOutcome>();
    const flow = createCleanupFlow(deps({ clean: () => pending.promise }));
    await flow.refresh();
    const clean = flow.cleanAll();
    flow.dispose();
    pending.resolve(completed('game:a'));
    await clean;
    expect(flow.proposals).toHaveLength(1);
    expect(flow.outcomes).toEqual({});
  });

  it('keeps unsupported items visible and Leaves only through the backend command', async () => {
    const unsupported = { ...proposal(), canClean: false };
    const dependencies = deps({
      list: () => Promise.resolve({ proposals: [unsupported], issues: [] }),
    });
    const flow = createCleanupFlow(dependencies);
    await flow.refresh();
    await flow.cleanAll();
    expect(dependencies.clean).not.toHaveBeenCalled();
    expect(flow.proposals).toHaveLength(1);
    await flow.leaveAll();
    expect(dependencies.leave).toHaveBeenCalledWith({ gameId: 'game:a', intent: 'intent:a' });
    expect(flow.proposals).toEqual([]);
  });

  it('retains a changed proposal when Leave is stale', async () => {
    const remaining = proposal('game:a', 'new-intent');
    const publishIssue = vi.fn<CleanupFlowDeps['publishIssue']>();
    const flow = createCleanupFlow(
      deps({
        publishIssue,
        leave: () =>
          Promise.resolve({
            gameId: 'game:a',
            status: 'stale',
            leaveRevision: null,
            remainingProposal: remaining,
            issue: { code: 'staleIntent', detail: null },
          }),
      }),
    );
    await flow.refresh();
    await flow.leaveAll();
    expect(flow.proposals).toEqual([remaining]);
    expect(flow.leaveIssues['game:a'].code).toBe('staleIntent');
    expect(publishIssue).not.toHaveBeenCalled();
  });

  it.each([
    {
      label: 'stale Clean',
      override: {
        clean: (): Promise<CleanupOutcome> =>
          Promise.resolve({
            gameId: 'game:a',
            status: 'stale',
            steps: [],
            remainingProposal: null,
            issue: { code: 'staleIntent', detail: null },
          }),
      },
      issue: { code: 'staleIntent', detail: null },
    },
    {
      label: 'partial Clean',
      override: {
        clean: (): Promise<CleanupOutcome> =>
          Promise.resolve({
            gameId: 'game:a',
            status: 'partial',
            steps: [
              {
                itemId: 'file:1',
                category: 'optiscalerFile',
                outcome: 'failed',
                issue: { code: 'operationFailed', detail: null },
              },
            ],
            remainingProposal: null,
            issue: { code: 'operationFailed', detail: null },
          }),
      },
      issue: { code: 'operationFailed', detail: null },
    },
    {
      label: 'stale Leave',
      override: {
        leave: (): Promise<LeaveOutcome> =>
          Promise.resolve({
            gameId: 'game:a',
            status: 'stale',
            leaveRevision: null,
            remainingProposal: null,
            issue: { code: 'staleIntent', detail: null },
          }),
      },
      issue: { code: 'staleIntent', detail: null },
    },
  ])('publishes an issue when $label retires the proposal', async ({ override, issue }) => {
    const publishIssue = vi.fn<CleanupFlowDeps['publishIssue']>();
    const flow = createCleanupFlow(deps({ ...override, publishIssue }));
    await flow.refresh();

    if ('clean' in override) {
      await flow.clean('game:a');
    } else {
      await flow.leave('game:a');
    }

    expect(flow.proposals).toEqual([]);
    expect(publishIssue).toHaveBeenCalledOnce();
    expect(publishIssue).toHaveBeenCalledWith(issue);
  });

  it.each(['clean', 'leave'] as const)(
    'does not publish an issue returned after disposal during %s',
    async (kind) => {
      const pendingClean = Promise.withResolvers<CleanupOutcome>();
      const pendingLeave = Promise.withResolvers<LeaveOutcome>();
      const publishIssue = vi.fn<CleanupFlowDeps['publishIssue']>();
      const flow = createCleanupFlow(
        deps({
          publishIssue,
          clean: () => pendingClean.promise,
          leave: () => pendingLeave.promise,
        }),
      );
      await flow.refresh();

      const action = kind === 'clean' ? flow.clean('game:a') : flow.leave('game:a');
      flow.dispose();
      if (kind === 'clean') {
        pendingClean.resolve({
          gameId: 'game:a',
          status: 'stale',
          steps: [],
          remainingProposal: null,
          issue: { code: 'staleIntent', detail: null },
        });
      } else {
        pendingLeave.resolve({
          gameId: 'game:a',
          status: 'stale',
          leaveRevision: null,
          remainingProposal: null,
          issue: { code: 'staleIntent', detail: null },
        });
      }
      await action;

      expect(publishIssue).not.toHaveBeenCalled();
    },
  );
});
