import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';

import type { ReshadeChannel } from '@entities/addon';

import type {
  AvailabilityReport,
  RenoDxInstallState,
  RenoDxUpdateReport,
  VulkanLayerManagementReport,
  VulkanLayerReport,
} from '../model/types';

/** Previews whether RenoDX can be installed for a game (loads/caches the manifest). */
export async function getRenoDxAvailability(gameId: string): Promise<AvailabilityReport> {
  return invokeDesktop<AvailabilityReport>('renodx_availability', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Installs RenoDX into a game and returns the resulting install state. Progress
 * is reported via `download-progress` events keyed by the game id. The requested
 * ReShade channel is sent explicitly; the backend decides which action is safe.
 */
export async function installRenoDx(
  gameId: string,
  reshadeChannel: ReshadeChannel,
  confirmAnticheat: boolean,
): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_install', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    reshadeChannel,
    confirmAnticheat,
  });
}

/**
 * Installs RenoDX into an external game from a user-downloaded add-on file, and
 * returns the resulting install state. Progress is reported via `download-progress`
 * events keyed by the game id (for the ReShade host, when one must be installed).
 */
export async function installRenoDxFromFile(
  gameId: string,
  filePath: string,
  reshadeChannel: ReshadeChannel,
  confirmAnticheat: boolean,
): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_install_from_file', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    filePath: requireNonBlankString(filePath, 'filePath'),
    reshadeChannel,
    confirmAnticheat,
  });
}

/** Uninstalls RenoDX from a game and returns the resulting install state. */
export async function uninstallRenoDx(gameId: string): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_uninstall', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Checks whether the installed RenoDX add-on for a game has an upstream update.
 * Never rejects on a network failure — it resolves to an `unknown` overall status.
 */
export async function checkRenoDxUpdate(gameId: string): Promise<RenoDxUpdateReport> {
  return invokeDesktop<RenoDxUpdateReport>('renodx_check_update', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Applies an available RenoDX add-on update for a game (re-fetch + atomic
 * in-place replace) and returns the resulting install state.
 */
export async function updateRenoDx(gameId: string): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_update', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/** Switches a recorded ReShade host binary between stable and nightly. */
export async function switchRenoDxReshadeChannel(
  gameId: string,
  reshadeChannel: ReshadeChannel,
): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_switch_reshade_channel', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    reshadeChannel,
  });
}

/**
 * Installs the DLSS-Fix companion add-on for a game that already has RenoDX.
 * Reports download progress via `download-progress` events keyed by the game id.
 */
export async function installRenoDxDlssFix(gameId: string): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_install_dlss_fix', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/** Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact. */
export async function uninstallRenoDxDlssFix(gameId: string): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_uninstall_dlss_fix', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/** Returns whether a DLSS-Fix can be installed for this game. */
export async function getRenoDxDlssFixAvailability(gameId: string): Promise<boolean> {
  return invokeDesktop<boolean>('renodx_dlss_fix_availability', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Returns the shared ReShade Vulkan layer report. Action permissions are authored
 * by the backend; the UI must not infer them from detection or path facts.
 */
export async function getVulkanLayerStatus(): Promise<VulkanLayerReport> {
  return invokeDesktop<VulkanLayerReport>('renodx_vulkan_layer_status', {});
}

/** Returns the settings-facing shared Vulkan layer management report. */
export async function getVulkanLayerManagementStatus(): Promise<VulkanLayerManagementReport> {
  return invokeDesktop<VulkanLayerManagementReport>('renodx_vulkan_layer_management_status', {});
}

/** Applies the shared ReShade Vulkan layer for the selected settings channel. */
export async function applyVulkanLayer(
  reshadeChannel: ReshadeChannel,
): Promise<VulkanLayerManagementReport> {
  return invokeDesktop<VulkanLayerManagementReport>('renodx_apply_vulkan_layer', {
    reshadeChannel,
  });
}

/**
 * Requests removal of the shared ReShade Vulkan layer. The backend only exposes
 * this action when it can perform it safely.
 */
export async function removeVulkanLayer(): Promise<VulkanLayerReport> {
  return invokeDesktop<VulkanLayerReport>('renodx_remove_vulkan_layer', {});
}

/** The set of RenoDX backend calls, injectable for testing. */
export type RenoDxApi = {
  getAvailability: typeof getRenoDxAvailability;
  install: typeof installRenoDx;
  installFromFile: typeof installRenoDxFromFile;
  uninstall: typeof uninstallRenoDx;
  checkUpdate: typeof checkRenoDxUpdate;
  update: typeof updateRenoDx;
  switchChannel: typeof switchRenoDxReshadeChannel;
  installDlssFix: typeof installRenoDxDlssFix;
  uninstallDlssFix: typeof uninstallRenoDxDlssFix;
  dlssFixAvailability: typeof getRenoDxDlssFixAvailability;
  vulkanLayerStatus: typeof getVulkanLayerStatus;
  vulkanLayerManagementStatus: typeof getVulkanLayerManagementStatus;
  applyVulkanLayer: typeof applyVulkanLayer;
  removeVulkanLayer: typeof removeVulkanLayer;
};

/** The default API bound to the real Tauri commands. */
export const renodxApi: RenoDxApi = {
  getAvailability: getRenoDxAvailability,
  install: installRenoDx,
  installFromFile: installRenoDxFromFile,
  uninstall: uninstallRenoDx,
  checkUpdate: checkRenoDxUpdate,
  update: updateRenoDx,
  switchChannel: switchRenoDxReshadeChannel,
  installDlssFix: installRenoDxDlssFix,
  uninstallDlssFix: uninstallRenoDxDlssFix,
  dlssFixAvailability: getRenoDxDlssFixAvailability,
  vulkanLayerStatus: getVulkanLayerStatus,
  vulkanLayerManagementStatus: getVulkanLayerManagementStatus,
  applyVulkanLayer,
  removeVulkanLayer,
};
