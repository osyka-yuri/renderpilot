import type { DllInfoDto, SettingStateResponse } from '@features/nvapi-settings';

export function displayDllLabel(dllInfo: DllInfoDto, unavailableLabel: string): string {
  if (dllInfo.version === null) {
    return unavailableLabel;
  }
  return dllInfo.manifest_label ?? `DLSS ${dllInfo.version}`;
}

export function isDllDependentCatalogBlocked(state: SettingStateResponse): boolean {
  return state.catalog_readiness === 'notReady' && state.dll_kind !== null;
}
