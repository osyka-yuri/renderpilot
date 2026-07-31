export type ExecutedD3d12ExecutableAction = {
  kind: 'patch' | 'restore';
  executable_path: string;
  from_sdk_version: number;
  to_sdk_version: number;
  original_sdk_version: number;
};

/** Exact JSON metadata emitted for operation summaries by CLI/API output DTOs. */
export type OperationMetadata = {
  game_name: string;
  technology: string;
  from_version: string | null;
  to_version: string;
  d3d12_executable_action?: ExecutedD3d12ExecutableAction;
};
