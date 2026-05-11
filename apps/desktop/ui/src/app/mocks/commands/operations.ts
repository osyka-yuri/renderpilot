import type { SwapPlan, ApplyOperationResult, RollbackOperationResult } from '@entities/operation';
import {
  mockState,
  BACKUP_SUFFIX,
  requireGameDetails,
  requireComponent,
  requireCandidateGroup,
  requireFirstComponentFile,
  requirePendingSwapPlan,
  findOperationTarget,
  findPrimaryRollbackComponent,
  updateCandidateGroupCurrentVersion,
  prependOperation,
  createOperationRecord,
  updateGameSummary,
  createPreviewOperationId,
  createRollbackOperationId,
} from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockBuildSwapPlan(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
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
    const operationId = createPreviewOperationId();

    const plan: SwapPlan = {
      operation_id: operationId,
      confirmation_token: `preview-token:${operationId}`,
      game_id: normalizedGameId,
      operation_type: 'replace_component',
      target_path: candidateGroup.file_path,
      replacement_path: candidate.file_path,
      original_version: candidateGroup.current_version ?? sourceFile.version ?? null,
      replacement_version: candidate.version ?? null,
      original_sha256: sourceFile.sha256,
      replacement_sha256: null,
      risk_level: candidate.warning ? 'medium' : 'low',
      requires_backup: true,
      requires_elevation: false,
      artifact_id: normalizedArtifactId,
      blockers: [],
      warnings: candidate.warning
        ? [candidate.warning]
        : ['confirmation_required_for_swappability'],
    };

    mockState.pendingPlansByOperationId.set(operationId, {
      plan,
      componentId: normalizedComponentId,
    });

    return clone(plan);
  });
}

export function mockApplyOperationPlan(
  operationId: string,
  confirmationToken: string,
): Promise<ApplyOperationResult> {
  return resolveMock(() => {
    const normalizedOperationId = requireNonEmptyText(operationId, 'operation id');
    const normalizedConfirmationToken = requireNonEmptyText(
      confirmationToken,
      'confirmation token',
    );

    const pending = requirePendingSwapPlan(normalizedOperationId, normalizedConfirmationToken);
    const { plan, componentId } = pending;

    const details = requireGameDetails(plan.game_id);
    const component = requireComponent(details, componentId);
    const sourceFile = requireFirstComponentFile(component);

    const now = Date.now();
    const backupPath = `${plan.target_path}${BACKUP_SUFFIX}`;

    mockState.appliedOperationsById.set(normalizedOperationId, {
      gameId: plan.game_id,
      componentId,
      targetPath: plan.target_path,
      originalVersion: plan.original_version ?? null,
      originalSha256: plan.original_sha256 ?? null,
      backupPath,
    });

    sourceFile.version = plan.replacement_version ?? sourceFile.version;
    sourceFile.sha256 = plan.replacement_sha256 ?? sourceFile.sha256;

    updateCandidateGroupCurrentVersion(details, componentId, sourceFile.version ?? null);

    prependOperation(
      details,
      createOperationRecord({
        operationId: normalizedOperationId,
        kind: 'replace_component',
        status: 'completed',
        createdAt: now - 60_000,
        completedAt: now,
        itemCount: 1,
        backupCount: 1,
        backupStatus: 'available',
      }),
    );

    mockState.pendingPlansByOperationId.delete(normalizedOperationId);

    updateGameSummary(plan.game_id, {
      backup_available: true,
      last_operation_status: 'completed',
      operation_count: details.operations.length,
    });

    const result: ApplyOperationResult = {
      operation_id: normalizedOperationId,
      game_id: plan.game_id,
      status: 'completed',
      completed_at: now,
      items: [
        {
          backup_id: `backup:${normalizedOperationId}`,
          component_id: component.id,
          applied_path: plan.target_path,
          replacement_path: plan.replacement_path,
          backup_path: backupPath,
        },
      ],
    };

    return clone(result);
  });
}

export function mockRollbackOperation(operationId: string): Promise<RollbackOperationResult> {
  return resolveMock(() => {
    const normalizedOperationId = requireNonEmptyText(operationId, 'operation id');

    if (mockState.rolledBackOperationIds.has(normalizedOperationId)) {
      throw new Error(
        `Mock preview operation ${normalizedOperationId} has already been rolled back.`,
      );
    }

    const target = findOperationTarget(normalizedOperationId);

    if (!target) {
      throw new Error(
        `Mock preview could not find operation ${normalizedOperationId} to rollback.`,
      );
    }

    if (target.operation.kind === 'rollback_operation') {
      throw new Error(`Mock preview cannot rollback rollback operation ${normalizedOperationId}.`);
    }

    const { details } = target;
    const snapshot = mockState.appliedOperationsById.get(normalizedOperationId);

    const component = snapshot
      ? requireComponent(details, snapshot.componentId)
      : findPrimaryRollbackComponent(details);

    if (!component) {
      throw new Error(
        `Mock preview rollback requires at least one component with files for ${details.game.identity.id}.`,
      );
    }

    const sourceFile = requireFirstComponentFile(component);

    if (snapshot) {
      sourceFile.version = snapshot.originalVersion ?? sourceFile.version;
      sourceFile.sha256 = snapshot.originalSha256 ?? sourceFile.sha256;

      updateCandidateGroupCurrentVersion(details, snapshot.componentId, snapshot.originalVersion);
    }

    const now = Date.now();
    const rollbackOperationId = createRollbackOperationId(normalizedOperationId);
    const restoredPath = snapshot?.targetPath ?? sourceFile.path;
    const backupPath = snapshot?.backupPath ?? `${sourceFile.path}${BACKUP_SUFFIX}`;

    prependOperation(
      details,
      createOperationRecord({
        operationId: rollbackOperationId,
        kind: 'rollback_operation',
        status: 'rolled_back',
        createdAt: now - 20_000,
        completedAt: now,
        itemCount: 1,
        backupCount: 1,
        backupStatus: 'available',
      }),
    );

    mockState.rolledBackOperationIds.add(normalizedOperationId);
    mockState.appliedOperationsById.delete(normalizedOperationId);

    updateGameSummary(details.game.identity.id, {
      backup_available: true,
      last_operation_status: 'rolled_back',
      operation_count: details.operations.length,
    });

    const result: RollbackOperationResult = {
      operation_id: rollbackOperationId,
      game_id: details.game.identity.id,
      status: 'rolled_back',
      completed_at: now,
      items: [
        {
          backup_id: `backup:${normalizedOperationId}`,
          component_id: component.id,
          restored_path: restoredPath,
          backup_path: backupPath,
        },
      ],
    };

    return clone(result);
  });
}
