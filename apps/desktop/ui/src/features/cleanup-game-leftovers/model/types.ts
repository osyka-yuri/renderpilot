export type LeftoverCategory =
  | 'vulkanRegistration'
  | 'engineConfig'
  | 'optiscalerFile'
  | 'addonFile'
  | 'componentFile'
  | 'privateCustody'
  | 'directory';

export type LeftoverIssueCode =
  | 'unsupportedOwner'
  | 'unprovenOwnership'
  | 'modifiedFile'
  | 'privateOriginalCustody'
  | 'foreignPending'
  | 'pendingConflict'
  | 'unavailableTarget'
  | 'nativeAuthorityUnavailable'
  | 'activeOwnershipConflict'
  | 'staleIntent'
  | 'operationFailed';

export type LeftoverIssue = { code: LeftoverIssueCode; detail: string | null };

export type LeftoverItem = {
  itemId: string;
  category: LeftoverCategory;
  path: string | null;
  disposition: 'cleanable' | 'retryable' | 'blocked';
  issue: LeftoverIssue | null;
};

export type LeftoverProposal = {
  gameId: string;
  gameName: string;
  installPath: string;
  rootState: 'missing' | 'residueOnly' | 'retiredEmpty';
  intent: string;
  leaveRevision: string;
  items: LeftoverItem[];
  canClean: boolean;
};

export type ListRetiredGameLeftoversOutput = {
  proposals: LeftoverProposal[];
  issues: { gameId: string | null; issue: LeftoverIssue }[];
};

export type RetiredGameLeftoversRequest = { gameId: string; intent: string };

export type CleanupStep = {
  itemId: string;
  category: LeftoverCategory;
  outcome:
    | 'removed'
    | 'released'
    | 'alreadyAbsent'
    | 'recovered'
    | 'preservedForeign'
    | 'blocked'
    | 'failed';
  issue: LeftoverIssue | null;
};

export type CleanupOutcome = {
  gameId: string;
  status: 'complete' | 'partial' | 'blocked' | 'stale';
  steps: CleanupStep[];
  remainingProposal: LeftoverProposal | null;
  issue: LeftoverIssue | null;
};

export type LeaveOutcome = {
  gameId: string;
  status: 'left' | 'stale';
  leaveRevision: string | null;
  remainingProposal: LeftoverProposal | null;
  issue: LeftoverIssue | null;
};
