import type { LumaManagedDependencySummary } from './types';

export type DgVoodooRequirement = Extract<LumaManagedDependencySummary, { kind: 'dgvoodoo2' }>;

export function dgVoodooRequirement(
  requirement: LumaManagedDependencySummary | null,
): DgVoodooRequirement | null {
  return requirement?.kind === 'dgvoodoo2' ? requirement : null;
}
