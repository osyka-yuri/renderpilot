import { humanizeToken } from '@shared/text';
import { fileNameFromPath } from '@shared/path';

export type LibraryVendorKey = 'nvidia' | 'amd' | 'intel' | 'microsoft' | 'other';

export type PresentedLibrary = {
  tag: string;
  label: string;
  vendorKey: LibraryVendorKey;
};

type LibraryFilePathLike = {
  path: string;
};

export const libraryVendorOrder: readonly LibraryVendorKey[] = ['nvidia', 'amd', 'intel', 'microsoft', 'other'];

const LIBRARY_UPPERCASE_WORDS = new Set([
  'AMD',
  'DLSS',
  'FG',
  'FSR',
  'NVAPI',
  'RR',
  'XEFG',
  'XELL',
  'XESS',
]);

const CANONICAL_LIBRARY_LABELS: Readonly<Record<string, string>> = {
  dlss_super_resolution: 'DLSS Super Resolution',
  dlss_frame_generation: 'DLSS Frame Generation',
  dlss_ray_reconstruction: 'DLSS Ray Reconstruction',
  nvidia_streamline: 'NVIDIA Streamline',
  nvidia_reflex: 'NVIDIA Reflex',
  intel_xess: 'Intel XeSS',
  intel_xefg: 'Intel XeFG',
  intel_xell: 'Intel Xe Low Latency',
  amd_fsr: 'AMD FSR',
  amd_fsr_upscaler: 'AMD FSR Upscaler',
  amd_fsr_frame_generation: 'AMD FSR Frame Generation',
  amd_fsr_ray_regeneration: 'AMD FSR Ray Regeneration',
  amd_fsr_loader: 'AMD FSR Loader',
  amd_fsr_radiance_cache: 'AMD FSR Radiance Cache',
  direct_storage: 'Microsoft DirectStorage',
};

/** Sub-tags that are internal to AMD FSR and expanded from the top-level `amd_fsr` alias. */
export const AMD_FSR_ALIAS_TAGS: readonly string[] = [
  'amd_fsr_upscaler',
  'amd_fsr_loader',
  'amd_fsr_radiance_cache',
];

export const ALL_KNOWN_LIBRARIES: readonly string[] = Object.keys(CANONICAL_LIBRARY_LABELS).filter(
  (key) => !AMD_FSR_ALIAS_TAGS.includes(key),
);

const COMPACT_LIBRARY_LABELS: Readonly<Record<string, string>> = {
  'DLSS Super Resolution': 'DLSS SR',
  'DLSS Frame Generation': 'DLSS FG',
  'DLSS Ray Reconstruction': 'DLSS RR',
  'NVIDIA Streamline': 'Streamline',
  'NVIDIA Reflex': 'Reflex',
  'Intel XeSS': 'XeSS',
  'Intel XeFG': 'XeFG',
  'Intel Xe Low Latency': 'XeLL',
  'AMD FSR': 'FSR',
  'AMD FSR Frame Generation': 'FSR FG',
  'AMD FSR Ray Regeneration': 'FSR RR',
  'Microsoft DirectStorage': 'DirectStorage',
};

const AMD_FSR_TECHNOLOGY = 'amd_fsr';
const AMD_FSR_ENTRY_POINT_FILE = 'amd_fidelityfx_dx12.dll';

const VENDOR_BLUEPRINTS: readonly {
  key: LibraryVendorKey;
  label: string;
}[] = [
  { key: libraryVendorOrder[0], label: 'NVIDIA' },
  { key: libraryVendorOrder[1], label: 'AMD' },
  { key: libraryVendorOrder[2], label: 'Intel' },
  { key: libraryVendorOrder[3], label: 'Microsoft' },
  { key: libraryVendorOrder[4], label: 'Additional' },
];

export function formatCanonicalLibraryLabel(value?: string | null): string {
  if (!value) {
    return 'Unknown';
  }

  return CANONICAL_LIBRARY_LABELS[value] ?? humanizeToken(value, LIBRARY_UPPERCASE_WORDS);
}

export function formatCompactLibraryLabel(value?: string | null): string {
  const canonicalLabel = formatCanonicalLibraryLabel(value);

  return COMPACT_LIBRARY_LABELS[canonicalLabel] ?? canonicalLabel;
}

export function isKnownLibrary(value?: string | null): value is string {
  if (!value) {
    return false;
  }

  const normalized = value.trim().toLowerCase();

  return normalized.length > 0 && normalized !== 'unknown';
}

export function libraryVendorKey(value?: string | null): LibraryVendorKey {
  const normalized = normalizeVendorValue(value);

  if (normalized.startsWith('dlss') || normalized.startsWith('nvidia')) {
    return 'nvidia';
  }

  if (normalized.startsWith('amd')) {
    return 'amd';
  }

  if (normalized.startsWith('intel')) {
    return 'intel';
  }

  if (normalized.startsWith('microsoft') || normalized.startsWith('directstorage')) {
    return 'microsoft';
  }

  return 'other';
}

export function vendorLabelForLibraryVendorKey(value: LibraryVendorKey): string {
  return VENDOR_BLUEPRINTS.find((vendor) => vendor.key === value)?.label ?? 'Additional';
}

export function comparePresentedLibraries(
  left: Pick<PresentedLibrary, 'vendorKey' | 'label' | 'tag'>,
  right: Pick<PresentedLibrary, 'vendorKey' | 'label' | 'tag'>,
): number {
  if (left.vendorKey !== right.vendorKey) {
    return vendorIndex(left.vendorKey) - vendorIndex(right.vendorKey);
  }

  const labelOrder = left.label.localeCompare(right.label, 'en', { sensitivity: 'base' });

  if (labelOrder !== 0) {
    return labelOrder;
  }

  return left.tag.localeCompare(right.tag, 'en', { sensitivity: 'base' });
}

export function createPresentedLibrary(
  tag?: string | null,
  formatLabel: (value?: string | null) => string = formatCompactLibraryLabel,
): PresentedLibrary | null {
  if (!isKnownLibrary(tag)) {
    return null;
  }

  const normalizedTag = tag.trim();

  return {
    tag: normalizedTag,
    label: formatLabel(normalizedTag),
    vendorKey: libraryVendorKey(normalizedTag),
  };
}

export function createPresentedLibraries(
  tags: readonly (string | null | undefined)[],
  formatLabel: (value?: string | null) => string = formatCompactLibraryLabel,
): PresentedLibrary[] {
  const librariesByTag = new Map<string, PresentedLibrary>();

  for (const tag of tags) {
    const library = createPresentedLibrary(tag, formatLabel);

    if (library && !librariesByTag.has(library.tag)) {
      librariesByTag.set(library.tag, library);
    }
  }

  return Array.from(librariesByTag.values()).sort(comparePresentedLibraries);
}

export function displayLibraryFilePath(
  technology: string | null | undefined,
  files: readonly LibraryFilePathLike[],
): string | null {
  if (technology === AMD_FSR_TECHNOLOGY) {
    const entryPoint = files.find(
      (file) => fileNameFromPath(file.path).toLowerCase() === AMD_FSR_ENTRY_POINT_FILE,
    );

    if (entryPoint) {
      return entryPoint.path;
    }
  }

  return files[0]?.path ?? null;
}

function normalizeVendorValue(value?: string | null): string {
  return formatCanonicalLibraryLabel(value).toLowerCase().replace(/[_\s]/g, '');
}

function vendorIndex(value: LibraryVendorKey): number {
  return libraryVendorOrder.indexOf(value);
}
