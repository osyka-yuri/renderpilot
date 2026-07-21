import { describe, expect, it } from 'vitest';

import { installedSelectionValue } from './version-selection';

describe('installedSelectionValue', () => {
  it('stays distinct from every selectable artifact id', () => {
    const artifactIds = [
      'artifact:same-primary-hash',
      'installed:component:openvr:0',
      'installed:component:openvr:1',
    ];

    expect(installedSelectionValue('component:openvr', artifactIds)).toBe(
      'installed:component:openvr:2',
    );
  });
});
