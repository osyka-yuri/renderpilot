import { describe, expect, it, vi } from 'vitest';

import type { Locale } from './locale-model';
import { createLocalePackLoader } from './locale-pack-loader';
import type { LocaleLoader, LocalePack } from './packs/types';
import { pack } from './runtime.test-support';

function createLoaders(
  overrides: Partial<Record<Locale, LocaleLoader>> = {},
): Record<Locale, LocaleLoader> {
  return {
    en: () => Promise.resolve(pack('en', { nav: 'Games' })),
    ru: () => Promise.resolve(pack('ru', { nav: 'Игры' })),
    es: () => Promise.resolve(pack('es', { nav: 'Juegos' })),
    fr: () => Promise.resolve(pack('fr', { nav: 'Jeux' })),
    de: () => Promise.resolve(pack('de', { nav: 'Spiele' })),
    ja: () => Promise.resolve(pack('ja', { nav: 'ゲーム' })),
    'zh-Hans': () => Promise.resolve(pack('zh-Hans', { nav: '游戏' })),
    'zh-Hant': () => Promise.resolve(pack('zh-Hant', { nav: '遊戲' })),
    ...overrides,
  };
}

describe('createLocalePackLoader', () => {
  it('deduplicates in-flight loads and caches successful packs', async () => {
    const russian = Promise.withResolvers<LocalePack>();
    const loader = vi.fn(() => russian.promise);
    const fallbackPack = pack('en', { nav: 'Games' });
    const repository = createLocalePackLoader(fallbackPack, createLoaders({ ru: loader }));

    expect(repository.getLoadedPack('en')).toBe(fallbackPack);
    const first = repository.loadPack('ru');
    const second = repository.loadPack('ru');
    expect(second).toBe(first);

    const russianPack = pack('ru', { nav: 'Игры' });
    russian.resolve(russianPack);
    await expect(first).resolves.toBe(russianPack);
    await expect(repository.loadPack('ru')).resolves.toBe(russianPack);
    expect(repository.getLoadedPack('ru')).toBe(russianPack);
    expect(loader).toHaveBeenCalledTimes(1);
  });

  it('registers the in-flight load before invoking the loader in a microtask', async () => {
    let reentrantLoad: Promise<LocalePack> | undefined;
    const russianPack = pack('ru', { nav: 'Игры' });
    const loader = vi.fn(() => {
      reentrantLoad = repository.loadPack('ru');
      return Promise.resolve(russianPack);
    });
    const repository = createLocalePackLoader(
      pack('en', { nav: 'Games' }),
      createLoaders({ ru: loader }),
    );

    const firstLoad = repository.loadPack('ru');
    expect(loader).not.toHaveBeenCalled();

    await Promise.resolve();
    expect(loader).toHaveBeenCalledOnce();
    expect(reentrantLoad).toBe(firstLoad);
    await expect(firstLoad).resolves.toBe(russianPack);
  });

  it('removes failed loads so a later request can retry', async () => {
    const loader = vi
      .fn<() => Promise<LocalePack>>()
      .mockRejectedValueOnce(new Error('missing chunk'))
      .mockResolvedValueOnce(pack('ru', { nav: 'Игры' }));
    const repository = createLocalePackLoader(
      pack('en', { nav: 'Games' }),
      createLoaders({ ru: loader }),
    );

    await expect(repository.loadPack('ru')).rejects.toThrow('missing chunk');
    await expect(repository.loadPack('ru')).resolves.toMatchObject({ locale: 'ru' });
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it.each([
    {
      name: 'malformed shape',
      candidate: { locale: 'ru', messages: {}, dynamicCatalogs: null },
    },
    {
      name: 'stale contract',
      candidate: { ...pack('ru', { nav: 'Игры' }), contractVersion: 'i18n-v1:stale' },
    },
  ])('rejects $name before caching', async ({ candidate }) => {
    const repository = createLocalePackLoader(
      pack('en', { nav: 'Games' }),
      createLoaders({
        ru: () => Promise.resolve(candidate as unknown as LocalePack),
      }),
    );

    await expect(repository.loadPack('ru')).rejects.toThrow('Invalid locale pack for "ru"');
    expect(repository.getLoadedPack('ru')).toBeUndefined();
  });

  it('validates a production pack in constant time without traversing messages', async () => {
    const opaqueMessages = new Proxy(
      {},
      {
        ownKeys: () => {
          throw new Error('message traversal is forbidden during pack validation');
        },
      },
    ) as LocalePack['messages'];
    const russianPack = { ...pack('ru', { nav: 'Игры' }), messages: opaqueMessages };
    const repository = createLocalePackLoader(
      pack('en', { nav: 'Games' }),
      createLoaders({ ru: () => Promise.resolve(russianPack) }),
    );

    await expect(repository.loadPack('ru')).resolves.toBe(russianPack);
  });
});
