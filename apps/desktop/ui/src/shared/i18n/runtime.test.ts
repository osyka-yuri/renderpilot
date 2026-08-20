import { describe, expect, it } from 'vitest';

import { createMessageRef } from './runtime.svelte';
import { createTestRuntime, pack } from './runtime.test-support';

describe('createI18nRuntime facade', () => {
  it('keeps translation reads on the active pack during a switch', async () => {
    const test = createTestRuntime({
      loaders: {
        ru: () => Promise.resolve(pack('ru', { nav: 'Игры' })),
      },
    });

    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Games',
    );
    await test.runtime.setLanguageMode('ru');
    expect(test.runtime.translateExternalMessage({ key: 'nav.games', fallback: 'fallback' })).toBe(
      'Игры',
    );
  });

  it('creates parameterless and parameterized message references without extra fields', () => {
    expect(createMessageRef('nav.games')).toEqual({ key: 'nav.games' });
    expect(createMessageRef('game.card.action.detailsLabel', { title: 'Control' })).toEqual({
      key: 'game.card.action.detailsLabel',
      params: { title: 'Control' },
    });
  });
});
