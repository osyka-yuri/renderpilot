// This feature is now headless: API + types only. The page-side composition
// (NvidiaProfileCard + the per-family DlssComponentCard) lives in
// `pages/game-details/ui` and owns the layout, since it is tightly bound to
// that page's tab structure.

// API
export {
  clearGameExecutableOverride,
  getDlssIndicatorState,
  getNvapiSettingState,
  listGameExecutableCandidates,
  listNvapiSettingStates,
  listNvapiSupportedSettings,
  revertNvapiSetting,
  setDlssIndicatorEnabled,
  setGameExecutableOverride,
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
