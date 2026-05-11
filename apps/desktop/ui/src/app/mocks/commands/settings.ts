import type { CatalogSettingPayload } from '@entities/settings';
import { mockState } from '../desktop-state';
import { requireNonEmptyText, resolveMock } from '../desktop-utils';

export function mockGetCatalogSetting(key: string): Promise<CatalogSettingPayload> {
  return resolveMock(() => {
    const normalizedKey = requireNonEmptyText(key, 'catalog setting key');

    return {
      value: mockState.catalogSettings.get(normalizedKey) ?? null,
    };
  });
}

export function mockSetCatalogSetting(key: string, value: string): Promise<{ saved: boolean }> {
  return resolveMock(() => {
    const normalizedKey = requireNonEmptyText(key, 'catalog setting key');

    if (value.trim().length === 0) {
      mockState.catalogSettings.delete(normalizedKey);
    } else {
      mockState.catalogSettings.set(normalizedKey, value);
    }

    return { saved: true };
  });
}
