// This feature is now headless: API + types only. The page-side composition
// (NvidiaProfileCard + DlssSrComponentCard) lives in `pages/game-details/ui`
// and owns the layout, since it is tightly bound to that page's tab structure.

// API
export {
  clearGameExecutableOverride,
  getNvapiSettingState,
  listGameExecutableCandidates,
  listNvapiSupportedSettings,
  revertNvapiSetting,
  setGameExecutableOverride,
  setNvapiSettingValue,
} from './api/desktop';

// Types
export type {
  BaselineDto,
  DllInfoDto,
  ExecutableCandidate,
  SettingDescriptor,
  SettingStateResponse,
  ValueDescriptor,
  ValueOption,
} from './model/types';
