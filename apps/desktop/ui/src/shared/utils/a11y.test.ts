import { describe, expect, it } from 'vitest';
import { ariaLabelUnlessLabelledBy, normalizeA11yTextProps } from '@shared/utils/a11y';

describe('a11y utils', () => {
  it('omits aria-label when aria-labelledby is present', () => {
    expect(ariaLabelUnlessLabelledBy('Standalone label', 'external-id')).toBeUndefined();
  });

  it('returns trimmed aria-label when labelled-by is missing', () => {
    expect(ariaLabelUnlessLabelledBy('  Search games  ', undefined)).toBe('Search games');
  });

  it('normalizes text props and trims empty values', () => {
    expect(
      normalizeA11yTextProps({
        label: '  Search  ',
        labelledBy: '   ',
        describedBy: 'desc-id',
        title: '  Tooltip  ',
      }),
    ).toEqual({
      ariaLabel: 'Search',
      ariaLabelledBy: undefined,
      ariaDescribedBy: 'desc-id',
      title: 'Tooltip',
    });
  });
});
