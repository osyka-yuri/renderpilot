import { describe, expect, it } from 'vitest';

import { bindExternalMessages, mergeExternalMessages } from './external';
import type { ExternalMessageCatalog } from './model';

describe('external message catalog construction', () => {
  it('binds translations to the exact reviewed source text', () => {
    expect(bindExternalMessages({ warning: 'Careful.' }, { warning: 'Осторожно.' })).toEqual({
      warning: { source: 'Careful.', translation: 'Осторожно.' },
    });
  });

  it('merges disjoint catalogs and rejects duplicate ownership', () => {
    const first = bindExternalMessages({ first: 'First.' }, { first: 'Первое.' });
    const second = bindExternalMessages({ second: 'Second.' }, { second: 'Второе.' });
    expect(mergeExternalMessages(first, second)).toEqual({ ...first, ...second });
    expect(() => mergeExternalMessages(first, first)).toThrow(
      'Duplicate external i18n message: first',
    );
  });

  it('rejects malformed sparse catalogs instead of hiding missing messages', () => {
    const malformed: ExternalMessageCatalog = { broken: undefined };
    expect(() => mergeExternalMessages(malformed)).toThrow('Invalid external i18n message: broken');
  });
});
