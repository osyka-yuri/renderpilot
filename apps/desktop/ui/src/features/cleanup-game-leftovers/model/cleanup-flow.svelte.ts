import type { PresentedError } from '@shared/error-presentation';
import type {
  CleanupOutcome,
  LeaveOutcome,
  LeftoverIssue,
  LeftoverProposal,
  ListRetiredGameLeftoversOutput,
  RetiredGameLeftoversRequest,
} from './types';

export type CleanupFlowDeps = {
  list: () => Promise<ListRetiredGameLeftoversOutput>;
  clean: (request: RetiredGameLeftoversRequest) => Promise<CleanupOutcome>;
  leave: (request: RetiredGameLeftoversRequest) => Promise<LeaveOutcome>;
  presentError: (error: unknown) => PresentedError;
  publishError: (error: unknown) => void;
  publishIssue: (issue: LeftoverIssue) => void;
};

export type CleanupFlow = ReturnType<typeof createCleanupFlow>;

/** Keeps one proposal snapshot and serializes explicit actions across games. */
export function createCleanupFlow(deps: CleanupFlowDeps) {
  let proposals = $state<LeftoverProposal[]>([]);
  let issues = $state<ListRetiredGameLeftoversOutput['issues']>([]);
  let outcomes = $state<Record<string, CleanupOutcome>>({});
  let errors = $state<Record<string, PresentedError>>({});
  let leaveIssues = $state<Record<string, LeftoverIssue>>({});
  let action = $state<'clean' | 'leave' | null>(null);
  let activeGameId = $state<string | null>(null);
  let loading = $state(false);
  let disposed = false;
  let generation = 0;
  let refreshRequested = false;
  let refreshPromise: Promise<void> | null = null;
  const isCurrent = (request: number): boolean => !disposed && request === generation;

  function refresh(): Promise<void> {
    if (disposed) {
      return Promise.resolve();
    }
    if (action !== null) {
      refreshRequested = true;
      return Promise.resolve();
    }
    if (refreshPromise !== null) {
      return refreshPromise;
    }
    const request = ++generation;
    loading = true;
    refreshPromise = Promise.resolve().then(async () => {
      try {
        const result = await deps.list();
        if (isCurrent(request)) {
          proposals = result.proposals;
          issues = result.issues;
          // Retain completed-step feedback for games still awaiting cleanup.
          const remainingIds = proposals.map((proposal) => proposal.gameId);
          outcomes = Object.fromEntries(
            Object.entries(outcomes).filter(([gameId]) => remainingIds.includes(gameId)),
          );
          errors = Object.fromEntries(
            Object.entries(errors).filter(([gameId]) => remainingIds.includes(gameId)),
          );
          leaveIssues = Object.fromEntries(
            Object.entries(leaveIssues).filter(([gameId]) => remainingIds.includes(gameId)),
          );
        }
      } catch (error) {
        if (isCurrent(request)) {
          deps.publishError(error);
        }
      } finally {
        if (isCurrent(request)) {
          loading = false;
        }
        refreshPromise = null;
      }
    });
    return refreshPromise;
  }

  function replaceProposal(gameId: string, remaining: LeftoverProposal | null): void {
    proposals = proposals.flatMap((proposal) =>
      proposal.gameId === gameId ? (remaining === null ? [] : [remaining]) : [proposal],
    );
  }

  async function runAction(kind: 'clean' | 'leave', gameIds: readonly string[]): Promise<void> {
    if (disposed || action !== null || loading) {
      return;
    }
    action = kind;
    const request = ++generation;
    try {
      for (const gameId of gameIds) {
        if (!isCurrent(request)) {
          break;
        }
        const proposal = proposals.find((candidate) => candidate.gameId === gameId);
        if (proposal === undefined || (kind === 'clean' && !proposal.canClean)) {
          continue;
        }
        activeGameId = gameId;
        errors = Object.fromEntries(Object.entries(errors).filter(([id]) => id !== gameId));
        leaveIssues = Object.fromEntries(
          Object.entries(leaveIssues).filter(([id]) => id !== gameId),
        );
        const payload = { gameId, intent: proposal.intent };
        try {
          if (kind === 'clean') {
            const result = await deps.clean(payload);
            if (!isCurrent(request)) {
              break;
            }
            outcomes = { ...outcomes, [gameId]: result };
            replaceProposal(gameId, result.remainingProposal);
            if (result.issue !== null && result.remainingProposal === null) {
              deps.publishIssue(result.issue);
            }
          } else {
            const result = await deps.leave(payload);
            if (!isCurrent(request)) {
              break;
            }
            replaceProposal(gameId, result.remainingProposal);
            if (result.issue !== null) {
              leaveIssues = { ...leaveIssues, [gameId]: result.issue };
            }
            if (result.issue !== null && result.remainingProposal === null) {
              deps.publishIssue(result.issue);
            }
          }
        } catch (error) {
          if (!isCurrent(request)) {
            break;
          }
          errors = { ...errors, [gameId]: deps.presentError(error) };
          // A failed transport may hide a completed commit or consumed intent.
          refreshRequested = true;
        }
      }
    } finally {
      if (isCurrent(request)) {
        action = null;
        activeGameId = null;
        if (refreshRequested) {
          refreshRequested = false;
          await refresh();
        }
      }
    }
  }

  function dispose(): void {
    disposed = true;
    generation++;
  }

  return {
    get proposals() {
      return proposals;
    },
    get issues() {
      return issues;
    },
    get outcomes() {
      return outcomes;
    },
    get errors() {
      return errors;
    },
    get leaveIssues() {
      return leaveIssues;
    },
    get busy() {
      return action !== null || loading;
    },
    get action() {
      return action;
    },
    get activeGameId() {
      return activeGameId;
    },
    refresh,
    clean: (gameId: string) => runAction('clean', [gameId]),
    leave: (gameId: string) => runAction('leave', [gameId]),
    cleanAll: () =>
      runAction(
        'clean',
        proposals.filter((proposal) => proposal.canClean).map((proposal) => proposal.gameId),
      ),
    leaveAll: () =>
      runAction(
        'leave',
        proposals.map((proposal) => proposal.gameId),
      ),
    dispose,
  };
}
