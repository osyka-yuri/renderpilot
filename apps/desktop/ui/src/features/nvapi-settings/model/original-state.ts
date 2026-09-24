import type { OriginalStateDto, ValueDescriptor } from './types';

/**
 * Whether restoring the recorded original would change the live DRS setting.
 * Explicit presence is part of the state: an explicit DWORD equal to the
 * driver default is different from an inherited default.
 */
export function canRestoreOriginal(
  original: OriginalStateDto | null,
  current: ValueDescriptor,
  currentIsExplicit: boolean | null,
): boolean {
  if (!original || currentIsExplicit === null) {
    return false;
  }

  if (original.present !== currentIsExplicit) {
    return true;
  }

  return original.present && original.value !== null && original.value.dword !== current.dword;
}
