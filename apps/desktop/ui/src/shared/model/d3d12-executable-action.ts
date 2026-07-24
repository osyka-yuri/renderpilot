/** Shared wire contract for a D3D12 Agility SDK executable transition. */
export type D3d12ExecutableAction = {
  kind: 'none' | 'patch' | 'restore' | 'repair_required';
  executable_path: string;
  backup_path: string;
  backup_exists: boolean;
  original_sdk_version: number;
  current_sdk_version: number;
  target_sdk_version: number;
  requires_confirmation: boolean;
};

/** EXE actions accepted by a destructive confirmation dialog. */
export type D3d12ExecutableMutationAction = Omit<D3d12ExecutableAction, 'kind'> & {
  kind: 'patch' | 'restore';
};

export function isD3d12ExecutableMutationAction(
  action: D3d12ExecutableAction | null | undefined,
): action is D3d12ExecutableMutationAction {
  return action?.kind === 'patch' || action?.kind === 'restore';
}
