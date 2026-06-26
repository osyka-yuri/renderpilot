import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';

import type { AvailabilityReport, RenoDxInstallState, RenoDxUpdateReport } from '../model/types';

/** Previews whether RenoDX can be installed for a game (loads/caches the manifest). */
export async function getRenoDxAvailability(gameId: string): Promise<AvailabilityReport> {
  return invokeDesktop<AvailabilityReport>('renodx_availability', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Installs RenoDX into a game and returns the resulting install state. Progress
 * is reported via `download-progress` events keyed by the game id. The ReShade
 * host, when one must be installed, is the nightly build.
 */
export async function installRenoDx(
  gameId: string,
  confirmAnticheat: boolean,
): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_install', {
    gameId: requireNonBlankString(gameId, 'gameId'),
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
  confirmAnticheat: boolean,
): Promise<RenoDxInstallState> {
  return invokeDesktop<RenoDxInstallState>('renodx_install_from_file', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    filePath: requireNonBlankString(filePath, 'filePath'),
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

/** The set of RenoDX backend calls, injectable for testing. */
export type RenoDxApi = {
  getAvailability: typeof getRenoDxAvailability;
  install: typeof installRenoDx;
  installFromFile: typeof installRenoDxFromFile;
  uninstall: typeof uninstallRenoDx;
  checkUpdate: typeof checkRenoDxUpdate;
  update: typeof updateRenoDx;
  installDlssFix: typeof installRenoDxDlssFix;
  uninstallDlssFix: typeof uninstallRenoDxDlssFix;
  dlssFixAvailability: typeof getRenoDxDlssFixAvailability;
};

/** The default API bound to the real Tauri commands. */
export const renodxApi: RenoDxApi = {
  getAvailability: getRenoDxAvailability,
  install: installRenoDx,
  installFromFile: installRenoDxFromFile,
  uninstall: uninstallRenoDx,
  checkUpdate: checkRenoDxUpdate,
  update: updateRenoDx,
  installDlssFix: installRenoDxDlssFix,
  uninstallDlssFix: uninstallRenoDxDlssFix,
  dlssFixAvailability: getRenoDxDlssFixAvailability,
};
