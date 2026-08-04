import { describe, expect, it } from 'vitest';

import { titleMonogram } from './presenters';

describe('titleMonogram', () => {
  it('keeps ordinary word selection and uppercase behavior', () => {
    expect(titleMonogram('render pilot desktop', 'en')).toBe('RP');
    expect(titleMonogram('portal', 'en')).toBe('PO');
  });

  it('treats an emoji ZWJ family as one grapheme', () => {
    expect(titleMonogram('👨‍👩‍👧‍👦 family', 'en')).toBe('👨‍👩‍👧‍👦F');
  });

  it('keeps combining marks attached to their letter', () => {
    expect(titleMonogram('e\u0301clair', 'en')).toBe('E\u0301C');
  });

  it('does not split surrogate pairs', () => {
    expect(titleMonogram('😀game', 'en')).toBe('😀G');
  });
});
