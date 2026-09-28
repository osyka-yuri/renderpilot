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

/** Identity of the executable change shown in a confirmation. */
export function d3d12ExecutableActionIdentity(action: D3d12ExecutableMutationAction): string {
  return JSON.stringify([
    action.kind,
    action.executable_path,
    action.backup_path,
    action.backup_exists,
    action.original_sdk_version,
    action.current_sdk_version,
    action.target_sdk_version,
  ]);
}

/** Shows each executable change once, regardless of backend token requirements. */
export function uniqueD3d12ExecutableMutationActions(
  actions: readonly D3d12ExecutableMutationAction[],
): D3d12ExecutableMutationAction[] {
  const seen = new Set<string>();
  return actions.filter((action) => {
    const identity = d3d12ExecutableActionIdentity(action);
    if (seen.has(identity)) {
      return false;
    }
    seen.add(identity);
    return true;
  });
}
