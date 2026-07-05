import { normalizeUniqueTrimmedStrings } from '@shared/text';
import { ADDON_DISPLAY_NAME, ALL_ADDON_KINDS } from '@shared/model';
import type { AddonCapability } from './types';

export const ALL_ADDON_CAPABILITIES: readonly AddonCapability[] = ALL_ADDON_KINDS;

const ADDON_CAPABILITY_SET: ReadonlySet<string> = new Set(ALL_ADDON_CAPABILITIES);

export function isAddonCapability(value: string): value is AddonCapability {
  return ADDON_CAPABILITY_SET.has(value);
}

export function normalizeAddonCapabilities(values: readonly string[]): AddonCapability[] {
  return normalizeUniqueTrimmedStrings(values).filter(isAddonCapability);
}

export function addonCapabilityLabel(capability: AddonCapability): string {
  return ADDON_DISPLAY_NAME[capability];
}

export function hasPartialAddonSelection(
  selected: readonly AddonCapability[],
  available: readonly AddonCapability[] = ALL_ADDON_CAPABILITIES,
): boolean {
  if (available.length === 0) {
    return false;
  }

  const availableSet = new Set(available);
  return selected.filter((capability) => availableSet.has(capability)).length < available.length;
}
