import type { Nullable, UnixTimestampMs, FilePath, Version } from '@shared/utils';
export type OperationId = string;
export type BackupId = string;
export type ConfirmationToken = string;
export type GameId = string;

export type RiskLevel = 'safe' | 'low' | 'medium' | 'high' | 'blocked' | 'unknown';

export type OperationStatus = string;
export type OperationKind = string;
export type BackupStatus = string;

export type SwapPlan = {
  operation_id: OperationId;
  confirmation_token: ConfirmationToken;

  game_id: GameId;
  operation_type: OperationKind;

  target_path: FilePath;
  replacement_path: FilePath;

  original_version?: Nullable<Version>;
  replacement_version?: Nullable<Version>;

  original_sha256?: Nullable<string>;
  replacement_sha256?: Nullable<string>;

  risk_level: RiskLevel;

  requires_backup: boolean;
  requires_elevation: boolean;

  artifact_id: string;

  blockers: string[];
  warnings: string[];
};

export type AppliedOperationItem = {
  backup_id: BackupId;
  component_id: string;
  applied_path: FilePath;
  replacement_path: FilePath;
  backup_path: FilePath;
};

export type RollbackOperationItem = {
  backup_id: BackupId;
  component_id: string;
  restored_path: FilePath;
  backup_path: FilePath;
};

export type ApplyOperationResult = {
  operation_id: OperationId;
  game_id: GameId;
  status: OperationStatus;
  completed_at?: Nullable<UnixTimestampMs>;
  items: AppliedOperationItem[];
};

export type RollbackOperationResult = {
  operation_id: OperationId;
  game_id: GameId;
  status: OperationStatus;
  completed_at?: Nullable<UnixTimestampMs>;
  items: RollbackOperationItem[];
};

export type OperationSummary = {
  operation_id: string;
  kind: string;
  status: string;
  created_at: number;
  completed_at?: number | null;
  item_count: number;
  backup_count: number;
  backup_status: string;
};

export type OperationHandler = (operationId: OperationId) => void;
