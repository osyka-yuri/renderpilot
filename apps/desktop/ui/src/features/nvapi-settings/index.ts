// API
export {
  clearGameExecutableOverride,
  getDlssIndicatorState,
  getNvapiSettingState,
  listGameExecutableCandidates,
  listGlobalNvapiSettingStates,
  listNvapiSettingStates,
  listNvapiSupportedSettings,
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
  DllInfoDto,
  DlssIndicatorState,
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
  type CreateDlssIndicatorContextOptions,
} from './model/create-dlss-indicator-context.svelte';

export {
  createNvapiSettingsStore,
  type NvapiSettingsStore,
  type CreateNvapiSettingsStoreOptions,
} from './model/create-nvapi-settings-store.svelte';

export {
  createGlobalNvidiaPresetsContext,
  type GlobalNvidiaPresetsContext,
  type CreateGlobalNvidiaPresetsContextOptions,
} from './model/create-global-nvidia-presets-context.svelte';

// UI
export { default as NvapiSettingRow } from './ui/NvapiSettingRow.svelte';
export { default as NvapiSettingGroup } from './ui/NvapiSettingGroup.svelte';
