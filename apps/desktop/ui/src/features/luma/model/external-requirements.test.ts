import { describe, expect, it } from 'vitest';

import { dgVoodooRequirement } from './external-requirements';
import type { LumaManagedDependencySummary } from './types';

const DGVOODOO: LumaManagedDependencySummary = {
  kind: 'dgvoodoo2',
  version: '2.87.3',
};

describe('external requirements', () => {
  it('narrows a dgVoodoo2 requirement', () => {
    expect(dgVoodooRequirement(DGVOODOO)?.version).toBe('2.87.3');
    expect(dgVoodooRequirement(null)).toBeNull();
  });
});
