import { normalizeUniqueTrimmedStrings } from '@shared/text';
import { ADDON_DISPLAY_NAME, ALL_ADDON_KINDS } from '@shared/model';
import { hasPartialNormalizedSelection } from './selection-predicates';
import type { AddonCapability } from './types';

export const ALL_ADDON_CAPABILITIES: readonly AddonCapability[] = ALL_ADDON_KINDS;

const ADDON_CAPABILITY_SET: ReadonlySet<string> = new Set(ALL_ADDON_CAPABILITIES);

export function isAddonCapability(value: string): value is AddonCapability {
  return ADDON_CAPABILITY_SET.has(value);
}

export function normalizeAddonCapabilities(values: readonly string[]): AddonCapability[] {
  return normalizeUniqueTrimmedStrings(values).filter(isAddonCapability);
}

/** Returns the supported capability set in stable product-defined order. */
export function canonicalAddonCapabilities(values: readonly string[]): readonly AddonCapability[] {
  const normalized = new Set(normalizeAddonCapabilities(values));
  return ALL_ADDON_CAPABILITIES.filter((capability) => normalized.has(capability));
}

export function addonCapabilityLabel(capability: AddonCapability): string {
  return ADDON_DISPLAY_NAME[capability];
}

export function hasPartialAddonSelection(
  selected: readonly string[],
  available: readonly string[] = ALL_ADDON_CAPABILITIES,
): boolean {
  return hasPartialNormalizedSelection(
    normalizeAddonCapabilities(selected),
    normalizeAddonCapabilities(available),
  );
}
