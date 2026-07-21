import type { ApplySwapResult, RollbackComponentResult } from '@entities/operation';
import {
  recordOperation,
  requireCandidateGroup,
  requireComponent,
  requireFirstComponentFile,
  requireGameDetails,
  updateCandidateGroupCurrentVersion,
  updateGameSummary,
} from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

let operationSequence = 0;

function nextOperationId(kind: string): string {
  operationSequence += 1;
  return `mock-op:${kind}:${operationSequence}`;
}

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
    const updatedFileCount = sourceComponent.files.length;
    const now = Date.now();

    sourceFile.version = candidate.version ?? sourceFile.version;
    sourceFile.sha256 = candidate.sha256 || sourceFile.sha256;

    updateCandidateGroupCurrentVersion(details, normalizedComponentId, sourceFile.version ?? null);

    recordOperation(details, {
      operation_id: nextOperationId('replace_component'),
      kind: 'replace_component',
      status: 'completed',
      created_at: now,
      completed_at: now,
      item_count: updatedFileCount,
      component_id: normalizedComponentId,
    });

    updateGameSummary(normalizedGameId, {
      rollback_available: true,
    });

    const result: ApplySwapResult = {
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      applied_path: candidateGroup.file_path,
      replacement_path: candidate.file_path ?? '',
      updated_file_count: updatedFileCount,
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
    const restoredFileCount = component.files.length;
    const now = Date.now();

    sourceFile.version = 'original-version';
    sourceFile.sha256 = 'original-sha256';

    updateCandidateGroupCurrentVersion(details, normalizedComponentId, 'original-version');

    recordOperation(details, {
      operation_id: nextOperationId('rollback_component'),
      kind: 'rollback_component',
      status: 'rolled_back',
      created_at: now,
      completed_at: now,
      item_count: restoredFileCount,
      component_id: normalizedComponentId,
    });

    updateGameSummary(normalizedGameId, {
      rollback_available: false,
    });

    const result: RollbackComponentResult = {
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      restored_path: sourceFile.path,
      restored_file_count: restoredFileCount,
    };

    return clone(result);
  });
}
