import { describe, expect, it } from 'vitest';

import { hasPartialAddonSelection } from './addon-capabilities';
import { hasPartialLauncherSelection } from './launcher-filters';
import { hasPartialLibrarySelection } from './library-filters';

describe('selection completeness predicates', () => {
  it('normalizes duplicate library values and ignores unknown selected values', () => {
    expect(
      hasPartialLibrarySelection(
        [' LibraryAlpha ', 'LibraryBeta', 'LibraryUnknown'],
        ['LibraryAlpha', 'LibraryAlpha', 'LibraryBeta'],
      ),
    ).toBe(false);
    expect(hasPartialLibrarySelection(['LibraryUnknown'], ['LibraryAlpha'])).toBe(true);
    expect(hasPartialLibrarySelection(['LibraryUnknown'], [])).toBe(false);
  });

  it('normalizes duplicate launcher values and ignores unknown selected values', () => {
    expect(
      hasPartialLauncherSelection([' Steam ', 'Epic', 'Unknown'], ['Steam', 'Steam', 'Epic']),
    ).toBe(false);
    expect(hasPartialLauncherSelection(['Unknown'], ['Steam'])).toBe(true);
    expect(hasPartialLauncherSelection(['Unknown'], [])).toBe(false);
  });

  it('normalizes addon values and preserves empty semantics', () => {
    expect(hasPartialAddonSelection([' luma ', 'unknown'], ['luma', ' luma '])).toBe(false);
    expect(hasPartialAddonSelection(['luma', 'unknown'], ['luma'])).toBe(false);
    expect(hasPartialAddonSelection(['unknown'], ['luma'])).toBe(true);
    expect(hasPartialAddonSelection(['luma'], ['luma', 'renodx'])).toBe(true);
    expect(hasPartialAddonSelection([], ['luma'])).toBe(true);
    expect(hasPartialAddonSelection([], [])).toBe(false);
  });
});
