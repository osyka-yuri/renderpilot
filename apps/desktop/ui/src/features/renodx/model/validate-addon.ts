import { createMessageRef, type MessageRef } from '@shared/i18n';
import { fileNameFromPath } from '@shared/path';

export type AddonArch = 'x64' | 'x86';

/** Upstream RenoDX add-on file extensions (architecture-specific). */
export const ADDON_EXTENSIONS = ['addon64', 'addon32'] as const;

/** Whether a path points at a RenoDX add-on file (by extension). */
export function isAddonFile(filePath: string): boolean {
  return addonArch(filePath) !== null;
}

/** An i18n message reference (key + optional params) for the UI to translate. */
export type AddonMessage = MessageRef;

export type AddonValidation = {
  fileName: string;
  /** A hard error that blocks the install (wrong extension or architecture). */
  error: AddonMessage | null;
  /** A soft warning that still allows the install (unexpected file name). */
  warning: AddonMessage | null;
};

const ARCH_LABEL: Record<AddonArch, string> = { x64: '64-bit', x86: '32-bit' };

/** The architecture a RenoDX add-on file targets, from its extension. */
export function addonArch(fileName: string): AddonArch | null {
  const lower = fileName.toLowerCase();
  if (lower.endsWith('.addon64')) {
    return 'x64';
  }
  if (lower.endsWith('.addon32')) {
    return 'x86';
  }
  return null;
}

/**
 * Tiered validation of a user-picked add-on file against a game:
 *   - wrong extension or architecture mismatch → hard error (blocks the install)
 *   - unexpected file name when the catalogue knows the expected one → warning
 *   - unknown game (no expected name) → no name warning (avoids a false positive)
 *
 * Pure (returns i18n references, not translated strings) so it is unit-testable.
 */
export function validateAddonFile(
  filePath: string,
  ctx: { gameArch: AddonArch | null; expectedAddonName: string | null },
): AddonValidation {
  const fileName = fileNameFromPath(filePath);
  const fileArch = addonArch(fileName);

  if (!fileArch) {
    return {
      fileName,
      error: createMessageRef('gameDetails.renodx.fileInstall.errorExtension'),
      warning: null,
    };
  }
  if (ctx.gameArch && ctx.gameArch !== fileArch) {
    return {
      fileName,
      error: createMessageRef('gameDetails.renodx.fileInstall.errorArch', {
        addon: ARCH_LABEL[fileArch],
        game: ARCH_LABEL[ctx.gameArch],
      }),
      warning: null,
    };
  }
  let warning: AddonMessage | null = null;
  if (
    ctx.expectedAddonName &&
    !fileName.toLowerCase().startsWith(ctx.expectedAddonName.toLowerCase())
  ) {
    warning = createMessageRef('gameDetails.renodx.fileInstall.warnName', {
      expected: ctx.expectedAddonName,
    });
  }
  return { fileName, error: null, warning };
}
