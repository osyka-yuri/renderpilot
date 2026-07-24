import type { GameCandidate } from '@entities/game';

export type CandidatePartition = {
  hasExecutableActions: boolean;
  compatible: GameCandidate[];
  changesExecutable: GameCandidate[];
  unavailable: GameCandidate[];
};

/** Partitions every candidate exactly once, including legacy entries without an EXE action. */
export function partitionD3d12Candidates(candidates: readonly GameCandidate[]): CandidatePartition {
  const hasExecutableActions = candidates.some(
    (candidate) => candidate.d3d12_executable_action !== null,
  );
  if (!hasExecutableActions) {
    return {
      hasExecutableActions: false,
      compatible: [...candidates],
      changesExecutable: [],
      unavailable: [],
    };
  }

  const compatible: GameCandidate[] = [];
  const changesExecutable: GameCandidate[] = [];
  const unavailable: GameCandidate[] = [];
  for (const candidate of candidates) {
    const kind = candidate.d3d12_executable_action?.kind;
    switch (kind) {
      case undefined:
      case 'none':
        compatible.push(candidate);
        break;
      case 'patch':
      case 'restore':
        changesExecutable.push(candidate);
        break;
      case 'repair_required':
        unavailable.push(candidate);
        break;
      default:
        kind satisfies never;
    }
  }

  return { hasExecutableActions, compatible, changesExecutable, unavailable };
}
