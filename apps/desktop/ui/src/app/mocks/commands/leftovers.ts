import type {
  CleanupOutcome,
  CleanupStep,
  LeaveOutcome,
  LeftoverProposal,
  ListRetiredGameLeftoversOutput,
  RetiredGameLeftoversRequest,
} from '@features/cleanup-game-leftovers';
import { clone } from '../desktop-utils';

let proposals: LeftoverProposal[] = [];
let intentSequence = 0;
const issued = new Map<string, string>();
const leftRevisions = new Map<string, string>();

/** Isolated preview fixtures; these commands never access the host filesystem. */
export function setRetiredGameLeftoversPreview(next: readonly LeftoverProposal[]): void {
  proposals = clone([...next]);
  issued.clear();
}

export function resetRetiredGameLeftoversPreview(): void {
  proposals = [];
  intentSequence = 0;
  issued.clear();
  leftRevisions.clear();
}

function issue(proposal: LeftoverProposal): LeftoverProposal {
  const result = { ...proposal, intent: `preview-leftovers-${++intentSequence}` };
  issued.set(result.gameId, result.intent);
  return clone(result);
}

export function mockListRetiredGameLeftovers(): Promise<ListRetiredGameLeftoversOutput> {
  issued.clear();
  return Promise.resolve({
    proposals: proposals
      .filter((proposal) => leftRevisions.get(proposal.gameId) !== proposal.leaveRevision)
      .map(issue),
    issues: [],
  });
}

function consume(request: RetiredGameLeftoversRequest): LeftoverProposal | undefined {
  if (issued.get(request.gameId) !== request.intent) {
    return undefined;
  }
  issued.delete(request.gameId);
  return proposals.find((proposal) => proposal.gameId === request.gameId);
}

function staleProposal(gameId: string): LeftoverProposal | null {
  const current = proposals.find((proposal) => proposal.gameId === gameId);
  return current === undefined ? null : issue(current);
}

export function mockCleanRetiredGameLeftovers(
  request: RetiredGameLeftoversRequest,
): Promise<CleanupOutcome> {
  const proposal = consume(request);
  if (proposal === undefined) {
    return Promise.resolve({
      gameId: request.gameId,
      status: 'stale',
      steps: [],
      remainingProposal: staleProposal(request.gameId),
      issue: { code: 'staleIntent', detail: null },
    });
  }
  const remaining = proposal.items.filter((item) => item.disposition === 'blocked');
  const steps: CleanupStep[] = proposal.items.map((item) => ({
    itemId: item.itemId,
    category: item.category,
    outcome:
      item.disposition === 'blocked'
        ? 'blocked'
        : item.category === 'engineConfig' || item.category === 'vulkanRegistration'
          ? 'released'
          : 'removed',
    issue: item.issue,
  }));
  const next =
    remaining.length === 0
      ? null
      : {
          ...proposal,
          items: remaining,
          canClean: false,
          leaveRevision: `${proposal.leaveRevision}-remaining`,
        };
  proposals = proposals.flatMap((candidate) =>
    candidate.gameId === request.gameId ? (next === null ? [] : [next]) : [candidate],
  );
  return Promise.resolve({
    gameId: request.gameId,
    status:
      next === null
        ? 'complete'
        : steps.some((step) => step.outcome !== 'blocked')
          ? 'partial'
          : 'blocked',
    steps,
    remainingProposal: next === null ? null : issue(next),
    issue: null,
  });
}

export function mockLeaveRetiredGameLeftovers(
  request: RetiredGameLeftoversRequest,
): Promise<LeaveOutcome> {
  const proposal = consume(request);
  if (proposal === undefined) {
    return Promise.resolve({
      gameId: request.gameId,
      status: 'stale',
      leaveRevision: null,
      remainingProposal: staleProposal(request.gameId),
      issue: { code: 'staleIntent', detail: null },
    });
  }
  leftRevisions.set(request.gameId, proposal.leaveRevision);
  return Promise.resolve({
    gameId: request.gameId,
    status: 'left',
    leaveRevision: proposal.leaveRevision,
    remainingProposal: null,
    issue: null,
  });
}
