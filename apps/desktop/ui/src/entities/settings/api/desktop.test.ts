import { beforeEach, describe, expect, it, vi } from 'vitest';
import { COVERS_STEAM_CDN_SETTING_KEY } from '../model/catalog-setting-keys';

const invokeDesktop = vi.hoisted(() => vi.fn());

vi.mock('@shared/api', () => ({ invokeDesktop }));

import { setCatalogBooleanSetting, setCatalogSetting } from './desktop';

describe('settings desktop boundary', () => {
  beforeEach(() => {
    invokeDesktop.mockReset();
  });

  it.each([
    [true, 'true'],
    [false, 'false'],
  ] as const)(
    'serializes boolean %s at the catalog transport boundary',
    async (value, expected) => {
      invokeDesktop.mockResolvedValueOnce({ saved: true });

      await setCatalogBooleanSetting(COVERS_STEAM_CDN_SETTING_KEY, value);

      expect(invokeDesktop).toHaveBeenCalledWith('set_catalog_setting', {
        key: COVERS_STEAM_CDN_SETTING_KEY,
        value: expected,
      });
    },
  );

  it('keeps the generic catalog setting boundary string-only', async () => {
    invokeDesktop.mockResolvedValueOnce({ saved: true });

    await setCatalogSetting('catalog.example', 'unchanged-string');

    expect(invokeDesktop).toHaveBeenCalledWith('set_catalog_setting', {
      key: 'catalog.example',
      value: 'unchanged-string',
    });
  });
});
