// API
export {
  clearGameExecutableOverride,
  createNvapiProfile,
  deleteNvapiProfile,
  getDlssIndicatorState,
  getNvapiSettingState,
  getNvapiProfileStatus,
  listGameExecutableCandidates,
  listGlobalNvapiSettingStates,
  listNvapiSettingStates,
  moveNvapiProfile,
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
  OriginalStateDto,
  CatalogReadiness,
  DllInfoDto,
  DlssIndicatorState,
  EffectiveExecutable,
  ExecutableCandidate,
  NvapiWarning,
  NvapiProfileStatus,
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
