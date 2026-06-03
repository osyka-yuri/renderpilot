export type SwapPlan = {
  game_id: string;
  component_id: string;
  artifact_id: string;
  target_path: string;
  replacement_path: string;
};

export type ApplySwapResult = {
  game_id: string;
  component_id: string;
  applied_path: string;
  replacement_path: string;
};

export type RollbackComponentResult = {
  game_id: string;
  component_id: string;
  restored_path: string;
};

export type KnownOperationStatus =
  | 'planned'
  | 'running'
  | 'completed'
  | 'failed'
  | 'blocked'
  | 'rolled_back'
  | 'cancelled';

export type KnownOperationKind = 'scan' | 'replace_component';

export type OperationStatus = KnownOperationStatus | (string & {});
export type OperationKind = KnownOperationKind | (string & {});

export type OperationSummary = {
  operation_id: string;
  kind: OperationKind;
  status: OperationStatus;
  created_at: number;
  completed_at?: number | null;
  item_count: number;
  component_id?: string;
};
