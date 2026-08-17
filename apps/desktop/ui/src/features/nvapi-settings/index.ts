// API
export {
  clearGameExecutableOverride,
  getDlssIndicatorState,
  getNvapiSettingState,
  listGameExecutableCandidates,
  listGlobalNvapiSettingStates,
  listNvapiSettingStates,
  listNvapiSupportedSettings,
  resolveGameExecutable,
  revertGlobalNvapiSetting,
  revertNvapiSetting,
  setDlssIndicatorEnabled,
  setGameExecutableOverride,
  setGlobalNvapiSettingValue,
  setNvapiSettingValue,
} from './api/desktop';

// Types
export type {
  BaselineDto,
  CatalogReadiness,
  DllInfoDto,
  DlssIndicatorState,
  EffectiveExecutable,
  ExecutableCandidate,
  NvapiWarning,
  SettingDescriptor,
  SettingFamily,
  SettingStateResponse,
  ValueDescriptor,
  ValueOption,
} from './model/types';

export {
  createDlssIndicatorContext,
  type DlssIndicatorContext,
} from './model/create-dlss-indicator-context.svelte';

export {
  createNvapiSettingsStore,
  type NvapiSettingsStore,
} from './model/create-nvapi-settings-store.svelte';

export {
  createGlobalNvidiaPresetsContext,
  type GlobalNvidiaPresetsContext,
} from './model/create-global-nvidia-presets-context.svelte';

// UI
export { default as NvapiSettingRow } from './ui/NvapiSettingRow.svelte';
export { default as NvapiSettingGroup } from './ui/NvapiSettingGroup.svelte';
