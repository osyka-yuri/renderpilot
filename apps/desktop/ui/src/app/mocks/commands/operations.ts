import type {
  ApplySwapResult,
  RollbackComponentResult,
  RollbackPlan,
  SwapPlan,
} from '@entities/operation';
import { DesktopCommandError, type CommandErrorCode } from '@shared/errors';
import type { D3d12ExecutableAction } from '@shared/model';
import {
  captureComponentBaseline,
  consumeComponentBaseline,
  hasComponentBaseline,
  nextMockOperationId,
  recordOperation,
  requireCandidateGroup,
  requireComponent,
  requireFirstComponentFile,
  requireGameDetails,
  updateCandidateGroupCurrentVersion,
  updateGameSummary,
} from '../desktop-state';
import { clone, requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockApplySwap(
  gameId: string,
  componentId: string,
  artifactId: string,
  confirmationToken: string | null,
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
    const executableAction = candidate.d3d12_executable_action;
    validateMockSwapConfirmation(
      normalizedGameId,
      normalizedComponentId,
      normalizedArtifactId,
      executableAction,
      confirmationToken,
    );

    const sourceFile = requireFirstComponentFile(sourceComponent);
    const updatedFileCount = sourceComponent.files.length;
    const now = Date.now();
    const executedAction = executableActionResult(executableAction);

    captureComponentBaseline(normalizedGameId, sourceComponent);
    sourceFile.version = candidate.technical_version ?? sourceFile.version;
    sourceFile.sha256 = candidate.sha256 || sourceFile.sha256;
    sourceComponent.rollback_available = true;

    updateCandidateGroupCurrentVersion(details, normalizedComponentId, sourceFile.version ?? null);
    if (executableAction?.kind === 'patch' || executableAction?.kind === 'restore') {
      applyMockExecutableAction(details, sourceComponent, executableAction);
    }

    recordOperation(details, {
      operation_id: nextMockOperationId('replace_component'),
      kind: 'replace_component',
      status: 'completed',
      created_at: now,
      completed_at: now,
      item_count: updatedFileCount,
      component_id: normalizedComponentId,
      metadata: null,
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
      d3d12_executable_action: executedAction,
    };

    return clone(result);
  });
}

export function mockPlanSwap(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');
    const normalizedComponentId = requireNonEmptyText(componentId, 'component id');
    const normalizedArtifactId = requireNonEmptyText(artifactId, 'artifact id');
    const details = requireGameDetails(normalizedGameId);
    const group = requireCandidateGroup(details, normalizedComponentId);
    const candidate = group.candidates.find((item) => item.artifact_id === normalizedArtifactId);
    if (!candidate) {
      throw new Error(
        `Mock preview could not find artifact ${normalizedArtifactId} for component ${normalizedComponentId}.`,
      );
    }
    const action = candidate.d3d12_executable_action;
    const token = action?.requires_confirmation
      ? mockConfirmationToken(normalizedGameId, normalizedComponentId, normalizedArtifactId, action)
      : `mock-plan:${normalizedComponentId}:${normalizedArtifactId}`;
    return clone({
      operation_id: `mock-plan:${normalizedComponentId}:${normalizedArtifactId}`,
      confirmation_token: token,
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      operation_type: 'replace_component',
      artifact_id: normalizedArtifactId,
      target_path: group.file_path,
      replacement_path: candidate.file_path ?? '',
      original_version:
        group.version_report.kind === 'known' ? group.version_report.technical_version : null,
      replacement_version: candidate.technical_version,
      original_sha256: null,
      replacement_sha256: candidate.sha256,
      risk_level: action?.kind === 'repair_required' ? 'blocked' : 'low',
      requires_elevation: false,
      blockers: action?.kind === 'repair_required' ? ['d3d12_executable_repair_required'] : [],
      warnings: [],
      files: [
        {
          action: 'replace',
          target_path: group.file_path,
          replacement_path: candidate.file_path,
          original_version: null,
          replacement_version: candidate.technical_version,
          original_sha256: null,
          replacement_sha256: candidate.sha256,
        },
      ],
      d3d12_executable_action: action,
    } satisfies SwapPlan);
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
    requireMockRollbackBaseline(normalizedGameId, component);
    const baseline = consumeComponentBaseline(normalizedGameId, normalizedComponentId);
    const restoredFileCount = baseline.length;
    const now = Date.now();
    const executableAction = mockManagedRollbackAction(component);
    const executedAction = executableActionResult(executableAction);

    component.files = baseline;
    component.rollback_available = false;

    const restoredFile = requireFirstComponentFile(component);
    updateCandidateGroupCurrentVersion(
      details,
      normalizedComponentId,
      restoredFile.version ?? null,
    );
    if (component.d3d12_executable_status) {
      const status = component.d3d12_executable_status;
      component.d3d12_executable_status = {
        ...status,
        status: 'original',
        selection_locked: false,
        backup_exists: false,
        current_sdk_version: status.original_sdk_version,
      };
      refreshMockExecutableActions(
        details,
        normalizedComponentId,
        status.original_sdk_version,
        false,
      );
    }

    recordOperation(details, {
      operation_id: nextMockOperationId('rollback_component'),
      kind: 'rollback_component',
      status: 'rolled_back',
      created_at: now,
      completed_at: now,
      item_count: restoredFileCount,
      component_id: normalizedComponentId,
      metadata: null,
    });

    updateGameSummary(normalizedGameId, {
      rollback_available: details.components.some((item) => item.rollback_available),
    });

    const result: RollbackComponentResult = {
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      restored_path: restoredFile.path,
      restored_file_count: restoredFileCount,
      d3d12_executable_action: executedAction,
    };

    return clone(result);
  });
}

export function mockPlanRollback(gameId: string, componentId: string): Promise<RollbackPlan> {
  return resolveMock(() => {
    const normalizedGameId = requireNonEmptyText(gameId, 'game id');
    const normalizedComponentId = requireNonEmptyText(componentId, 'component id');
    const details = requireGameDetails(normalizedGameId);
    const component = requireComponent(details, normalizedComponentId);
    requireMockRollbackBaseline(normalizedGameId, component);
    const affectedFiles = component.files.flatMap((file) => [file.path, `${file.path}.bak`]);
    const executableAction = mockManagedRollbackAction(component);
    if (executableAction) {
      affectedFiles.push(executableAction.executable_path, executableAction.backup_path);
    }
    return clone({
      game_id: normalizedGameId,
      component_id: normalizedComponentId,
      affected_files: Array.from(new Set(affectedFiles)).sort(),
      d3d12_executable_action: executableAction,
    } satisfies RollbackPlan);
  });
}

function requireMockRollbackBaseline(
  gameId: string,
  component: ReturnType<typeof requireComponent>,
): void {
  if (!component.rollback_available || !hasComponentBaseline(gameId, component.id)) {
    throw mockCommandError('invalid_argument');
  }
}

function validateMockSwapConfirmation(
  gameId: string,
  componentId: string,
  artifactId: string,
  action: D3d12ExecutableAction | null,
  confirmationToken: string | null,
): void {
  if (action?.kind === 'repair_required') {
    throw mockCommandError('invalid_argument');
  }
  if (
    action?.requires_confirmation &&
    confirmationToken !== mockConfirmationToken(gameId, componentId, artifactId, action)
  ) {
    throw mockCommandError('confirmation_token_mismatch');
  }
}

function mockConfirmationToken(
  gameId: string,
  componentId: string,
  artifactId: string,
  action: D3d12ExecutableAction,
): string {
  return [
    'mock-confirm',
    gameId,
    componentId,
    artifactId,
    action.kind,
    action.current_sdk_version,
    action.target_sdk_version,
    Number(action.backup_exists),
  ].join(':');
}

function executableActionResult(
  action: D3d12ExecutableAction | null,
): ApplySwapResult['d3d12_executable_action'] {
  if (action?.kind !== 'patch' && action?.kind !== 'restore') {
    return null;
  }
  return {
    kind: action.kind,
    executable_path: action.executable_path,
    from_sdk_version: action.current_sdk_version,
    to_sdk_version: action.target_sdk_version,
    original_sdk_version: action.original_sdk_version,
  };
}

function applyMockExecutableAction(
  details: ReturnType<typeof requireGameDetails>,
  component: ReturnType<typeof requireComponent>,
  action: D3d12ExecutableAction,
): void {
  const status = component.d3d12_executable_status;
  if (!status || (action.kind !== 'patch' && action.kind !== 'restore')) {
    throw mockCommandError('invalid_operation_state');
  }
  component.d3d12_executable_status = {
    ...status,
    status: action.target_sdk_version === action.original_sdk_version ? 'original' : 'patched',
    selection_locked: true,
    backup_exists: true,
    current_sdk_version: action.target_sdk_version,
  };
  refreshMockExecutableActions(details, component.id, action.target_sdk_version, true);
}

function refreshMockExecutableActions(
  details: ReturnType<typeof requireGameDetails>,
  componentId: string,
  currentSdkVersion: number,
  backupExists: boolean,
): void {
  const group = requireCandidateGroup(details, componentId);
  for (const candidate of group.candidates) {
    const action = candidate.d3d12_executable_action;
    if (!action) {
      continue;
    }
    const kind =
      action.target_sdk_version === currentSdkVersion
        ? 'none'
        : action.target_sdk_version === action.original_sdk_version
          ? 'restore'
          : 'patch';
    candidate.d3d12_executable_action = {
      ...action,
      kind,
      backup_exists: backupExists,
      current_sdk_version: currentSdkVersion,
      requires_confirmation:
        kind === 'restore' ||
        (kind === 'patch' && currentSdkVersion === action.original_sdk_version),
    };
  }
}

function mockManagedRollbackAction(
  component: ReturnType<typeof requireComponent>,
): D3d12ExecutableAction | null {
  const status = component.d3d12_executable_status;
  if (!status || !component.rollback_available) {
    return null;
  }
  return {
    kind:
      status.current_sdk_version === status.original_sdk_version ? 'none' : ('restore' as const),
    executable_path: status.executable_path,
    backup_path: status.backup_path,
    backup_exists: true,
    original_sdk_version: status.original_sdk_version,
    current_sdk_version: status.current_sdk_version,
    target_sdk_version: status.original_sdk_version,
    requires_confirmation: false,
  };
}

function mockCommandError(code: CommandErrorCode): DesktopCommandError {
  return DesktopCommandError.fromDto({ code });
}
