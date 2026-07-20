import type { MessageKey } from '@shared/i18n';

const INSTRUCTION_KEY_BY_LAUNCHER: Record<string, MessageKey | undefined> = {
  Steam: 'gameDetails.luma.launchArgs.instructions.steam',
  Gog: 'gameDetails.luma.launchArgs.instructions.gog',
  Epic: 'gameDetails.luma.launchArgs.instructions.epic',
  Ea: 'gameDetails.luma.launchArgs.instructions.ea',
  Ubisoft: 'gameDetails.luma.launchArgs.instructions.ubisoft',
};

export function launchArgsInstructionKey(launcher: string): MessageKey {
  return INSTRUCTION_KEY_BY_LAUNCHER[launcher] ?? 'gameDetails.luma.launchArgs.instructions.other';
}

/** A detected store can get a precise route; every other launch path uses the
 * neutral command-line guidance instead. */
export function hasKnownLaunchArgsInstructions(launcher: string): boolean {
  return INSTRUCTION_KEY_BY_LAUNCHER[launcher] !== undefined;
}

export function includesDx11LaunchArg(launchArgs: readonly string[]): boolean {
  return launchArgs.some((arg) => arg.trim().toLowerCase() === '-dx11');
}
