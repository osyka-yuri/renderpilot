import type { ApplySwapResult, RollbackComponentResult } from '@entities/operation';
import {
  requireGameDetails,
  requireComponent,
  requireCandidateGroup,
  requireFirstComponentFile,
  updateCandidateGroupCurrentVersion,
  updateGameSummary,
} from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockApplySwap(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<ApplySwapResult> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');
    const normalizedComponentId = requireNonEmptyText(componentId, 'component id');
    const normalizedArtifactId = requireNonEmptyText(artifactId, 'artifact id');

    const details = requireGameDetails(normalizedGameId);
    const sourceComponent = requireComponent(details, normalizedComponentId);
    const candidateGroup = requireCandidateGroup(details, normalizedComponentId);
    const candidate = candidateGroup.candidates.find(
      (item) => item.artifact_id === normalizedArtifactId,
    );

    if (!candidate) {
      throw new Error(
        `Mock preview could not find artifact ${normalizedArtifactId} for component ${normalizedComponentId}.`,
      );
    }

    const sourceFile = requireFirstComponentFile(sourceComponent);

    sourceFile.version = candidate.version ?? sourceFile.version;
    sourceFile.sha256 = candidate.file_path ?? sourceFile.sha256;

    updateCandidateGroupCurrentVersion(details, normalizedComponentId, sourceFile.version ?? null);

    updateGameSummary(normalizedGameId, {
      rollback_available: true,
      last_operation_status: 'completed',
      operation_count: details.operations.length,
    });

    const result: ApplySwapResult = {
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      applied_path: candidateGroup.file_path,
      replacement_path: candidate.file_path ?? '',
    };

    return clone(result);
  });
}

export function mockRollbackComponent(
  gameId: string,
  componentId: string,
): Promise<RollbackComponentResult> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');
    const normalizedComponentId = requireNonEmptyText(componentId, 'component id');

    const details = requireGameDetails(normalizedGameId);
    const component = requireComponent(details, normalizedComponentId);
    const sourceFile = requireFirstComponentFile(component);

    sourceFile.version = 'original-version';
    sourceFile.sha256 = 'original-sha256';

    updateCandidateGroupCurrentVersion(details, normalizedComponentId, 'original-version');

    updateGameSummary(normalizedGameId, {
      rollback_available: false,
      last_operation_status: 'rolled_back',
      operation_count: details.operations.length,
    });

    const result: RollbackComponentResult = {
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      restored_path: sourceFile.path,
    };

    return clone(result);
  });
}
