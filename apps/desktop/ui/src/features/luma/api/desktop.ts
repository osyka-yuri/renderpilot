import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';

import type { AvailabilityReport, LumaInstallState, LumaUpdateReport } from '../model/types';

/** Previews whether Luma can be installed for a game (loads/caches the manifest). */
export async function getLumaAvailability(gameId: string): Promise<AvailabilityReport> {
  return invokeDesktop<AvailabilityReport>('luma_availability', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Installs Luma into a game and returns the resulting install state. Progress is
 * reported via `download-progress` events keyed by the game id. Unlike RenoDX,
 * there is no channel parameter — Luma always installs the nightly ReShade host.
 */
export async function installLuma(
  gameId: string,
  gameContextToken?: string,
): Promise<LumaInstallState> {
  return invokeDesktop<LumaInstallState>('luma_install', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    ...(gameContextToken === undefined ? {} : { gameContextToken }),
  });
}

/** Uninstalls Luma from a game and returns the resulting install state. */
export async function uninstallLuma(gameId: string): Promise<LumaInstallState> {
  return invokeDesktop<LumaInstallState>('luma_uninstall', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/**
 * Checks whether the installed Luma add-on for a game has an upstream update.
 *
 * Catalogue/cache resolution and upstream HEAD/digest failures soft-fail to
 * overall `unknown` — this never rejects on network failure. Install/update
 * still hard-require a resolvable catalogue.
 *
 * `deep: true` may full-download archives. Passive probes keep `deep` false;
 * the backend still one-shot binds an unbound advisory release ZIP after
 * DB-loss adoption.
 */
export async function checkLumaUpdate(
  gameId: string,
  options?: { deep?: boolean },
): Promise<LumaUpdateReport> {
  return invokeDesktop<LumaUpdateReport>('luma_check_update', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    deep: options?.deep ?? false,
  });
}

/**
 * Applies an available Luma update for a game (re-fetch + set-diff apply) and
 * returns the resulting install state.
 *
 * Pass `forceFull: true` for Repair so the backend re-fetches the release ZIP
 * and runs a full set-diff even when the cheap ETag pre-check says current.
 */
export async function updateLuma(
  gameId: string,
  options?: { forceFull?: boolean; gameContextToken?: string },
): Promise<LumaInstallState> {
  return invokeDesktop<LumaInstallState>('luma_update', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    forceFull: options?.forceFull ?? false,
    ...(options?.gameContextToken === undefined
      ? {}
      : { gameContextToken: options.gameContextToken }),
  });
}

/** The set of Luma backend calls, injectable for testing. */
export type LumaApi = {
  getAvailability: typeof getLumaAvailability;
  install: typeof installLuma;
  uninstall: typeof uninstallLuma;
  checkUpdate: typeof checkLumaUpdate;
  update: typeof updateLuma;
};

/** The default API bound to the real Tauri commands. */
export const lumaApi: LumaApi = {
  getAvailability: getLumaAvailability,
  install: installLuma,
  uninstall: uninstallLuma,
  checkUpdate: checkLumaUpdate,
  update: updateLuma,
};
