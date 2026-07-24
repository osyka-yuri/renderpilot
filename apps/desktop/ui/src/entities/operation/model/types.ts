import type {
  D3d12ExecutableAction,
  ExecutedD3d12ExecutableAction as SharedExecutedD3d12ExecutableAction,
  OperationMetadata as SharedOperationMetadata,
} from '@shared/model';

export type SwapPlan = {
  operation_id: string;
  confirmation_token: string;
  game_id: string;
  component_id: string;
  operation_type: string;
  artifact_id: string;
  target_path: string;
  replacement_path: string;
  original_version: string | null;
  replacement_version: string | null;
  original_sha256: string | null;
  replacement_sha256: string | null;
  risk_level: string;
  requires_elevation: boolean;
  blockers: string[];
  warnings: string[];
  files: SwapPlanFile[];
  d3d12_executable_action: D3d12ExecutableAction | null;
};

export type SwapPlanFile = {
  action: string;
  target_path: string;
  replacement_path: string | null;
  original_version: string | null;
  replacement_version: string | null;
  original_sha256: string | null;
  replacement_sha256: string | null;
};

export type RollbackPlan = {
  game_id: string;
  component_id: string;
  affected_files: string[];
  d3d12_executable_action: D3d12ExecutableAction | null;
};

export type ApplySwapResult = {
  game_id: string;
  component_id: string;
  applied_path: string;
  replacement_path: string;
  updated_file_count: number;
  d3d12_executable_action: ExecutedD3d12ExecutableAction | null;
};

export type RollbackComponentResult = {
  game_id: string;
  component_id: string;
  restored_path: string;
  restored_file_count: number;
  d3d12_executable_action: ExecutedD3d12ExecutableAction | null;
};

export type ExecutedD3d12ExecutableAction = SharedExecutedD3d12ExecutableAction;

export type KnownOperationStatus =
  'planned' | 'running' | 'completed' | 'failed' | 'blocked' | 'rolled_back' | 'cancelled';

export type KnownOperationKind = 'scan' | 'replace_component' | 'rollback_component';

export type OperationStatus = KnownOperationStatus | (string & {});
export type OperationKind = KnownOperationKind | (string & {});

export type OperationMetadata = SharedOperationMetadata;

export type OperationSummary = {
  operation_id: string;
  kind: OperationKind;
  status: OperationStatus;
  created_at: number;
  completed_at: number | null;
  item_count: number;
  component_id: string;
  metadata: OperationMetadata | null;
};
