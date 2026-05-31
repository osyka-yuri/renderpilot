import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';

import type { ExecutableCandidate, SettingDescriptor, SettingStateResponse } from '../model/types';

export async function listNvapiSupportedSettings(gameId: string): Promise<SettingDescriptor[]> {
  return invokeDesktop<SettingDescriptor[]>('list_nvapi_supported_settings', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function listGameExecutableCandidates(gameId: string): Promise<ExecutableCandidate[]> {
  return invokeDesktop<ExecutableCandidate[]>('list_game_executable_candidates', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function setGameExecutableOverride(
  gameId: string,
  absolutePath: string,
): Promise<void> {
  await invokeDesktop('set_game_executable_override', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    absolutePath: requireNonBlankString(absolutePath, 'absolutePath'),
  });
}

export async function clearGameExecutableOverride(gameId: string): Promise<void> {
  await invokeDesktop('clear_game_executable_override', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function getNvapiSettingState(
  gameId: string,
  settingKey: string,
): Promise<SettingStateResponse> {
  return invokeDesktop<SettingStateResponse>('get_nvapi_setting_state', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    settingKey: requireNonBlankString(settingKey, 'settingKey'),
  });
}

/**
 * Reads the live state of every supported NVAPI setting for a game in one
 * backend call (a single DRS session), backing the grouped DLSS driver
 * settings card.
 */
export async function listNvapiSettingStates(gameId: string): Promise<SettingStateResponse[]> {
  return invokeDesktop<SettingStateResponse[]>('list_nvapi_setting_states', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function setNvapiSettingValue(
  gameId: string,
  settingKey: string,
  value: string,
): Promise<SettingStateResponse> {
  return invokeDesktop<SettingStateResponse>('set_nvapi_setting_value', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    settingKey: requireNonBlankString(settingKey, 'settingKey'),
    value: requireNonBlankString(value, 'value'),
  });
}

export async function revertNvapiSetting(
  gameId: string,
  settingKey: string,
  target: 'predefined' | 'baseline',
): Promise<SettingStateResponse> {
  return invokeDesktop<SettingStateResponse>('revert_nvapi_setting', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    settingKey: requireNonBlankString(settingKey, 'settingKey'),
    target,
  });
}
