import { invokeDesktop } from '@shared/api';
import { requireNonBlankString, requireString } from '@shared/validation';
import type { CatalogSettingPayload } from '../model/view-model';

export async function getCatalogSetting(key: string): Promise<CatalogSettingPayload> {
  return invokeDesktop<CatalogSettingPayload>('get_catalog_setting', {
    key: requireNonBlankString(key, 'key'),
  });
}

export async function setCatalogSetting(key: string, value: string): Promise<{ saved: boolean }> {
  return invokeDesktop<{ saved: boolean }>('set_catalog_setting', {
    key: requireNonBlankString(key, 'key'),
    value: requireString(value, 'value'),
  });
}
