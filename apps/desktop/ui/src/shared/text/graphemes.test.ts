import { describe, expect, it } from 'vitest';

import { takeGraphemePrefix } from './graphemes';

describe('takeGraphemePrefix', () => {
  it('takes complete Unicode graphemes', () => {
    expect(takeGraphemePrefix('👨‍👩‍👧‍👦family', 1, 'en')).toBe('👨‍👩‍👧‍👦');
    expect(takeGraphemePrefix('e\u0301clair', 1, 'fr')).toBe('e\u0301');
  });

  it('rejects invalid counts', () => {
    expect(takeGraphemePrefix('render', 0, 'en')).toBe('');
    expect(takeGraphemePrefix('render', 1.5, 'en')).toBe('');
  });
});
