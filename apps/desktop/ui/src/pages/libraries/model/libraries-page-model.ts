import { trimToEmpty } from '@shared/text';
import { t, getLocale } from '@shared/i18n';
import type { LibraryManifestEntry } from '@entities/library';

export type LibraryGroupKey =
  | 'dlss'
  | 'dlss_g'
  | 'dlss_d'
  | 'streamline'
  | 'fsr_31_dx12'
  | 'fsr_31_vk'
  | 'fsr_loader_dx12'
  | 'fsr_upscaler_dx12'
  | 'fsr_framegeneration_dx12'
  | 'fsr_denoiser_dx12'
  | 'fsr_radiancecache_dx12'
  | 'xess'
  | 'xess_dx11'
  | 'xess_fg'
  | 'xell'
  | 'other';

export type Vendor = 'nvidia' | 'amd' | 'intel';

export type VendorOption = Readonly<{
  value: Vendor;
  label: string;
}>;

export type LibraryTypeOption = Readonly<{
  value: string;
  label: string;
  groupKey: LibraryGroupKey;
}>;

export type LibraryTypeValue = (typeof typeOptionsByVendor)[Vendor][number]['value'];

const DEFAULT_GROUP_KEY: LibraryGroupKey = 'other';

export const vendorOptions = [
  { value: 'nvidia', label: 'NVIDIA' },
  { value: 'amd', label: 'AMD' },
  { value: 'intel', label: 'Intel' },
] as const satisfies readonly VendorOption[];

export const typeOptionsByVendor = {
  nvidia: [
    { value: 'dlss', label: 'DLSS', groupKey: 'dlss' },
    { value: 'dlss_fg', label: 'DLSS FG', groupKey: 'dlss_g' },
    { value: 'dlss_rr', label: 'DLSS RR', groupKey: 'dlss_d' },
    { value: 'streamline', label: 'Streamline', groupKey: 'streamline' },
  ],
  amd: [
    { value: 'fsr', label: 'FSR 3.1 DX12', groupKey: 'fsr_31_dx12' },
    { value: 'fsr_vk', label: 'FSR 3.1 VK', groupKey: 'fsr_31_vk' },
    { value: 'fsr_loader', label: 'FSR Loader', groupKey: 'fsr_loader_dx12' },
    { value: 'fsr_upscaler', label: 'FSR Upscaler', groupKey: 'fsr_upscaler_dx12' },
    { value: 'fsr_framegen', label: 'FSR FrameGen', groupKey: 'fsr_framegeneration_dx12' },
    { value: 'fsr_denoiser', label: 'FSR Denoiser', groupKey: 'fsr_denoiser_dx12' },
    { value: 'fsr_radiancecache', label: 'FSR RadianceCache', groupKey: 'fsr_radiancecache_dx12' },
  ],
  intel: [
    { value: 'xess', label: 'XeSS', groupKey: 'xess' },
    { value: 'xess_dx11', label: 'XeSS DX11', groupKey: 'xess_dx11' },
    { value: 'xefg', label: 'XeFG', groupKey: 'xess_fg' },
    { value: 'xell', label: 'XeLL', groupKey: 'xell' },
  ],
} as const satisfies Record<Vendor, readonly LibraryTypeOption[]>;

const vendorValues = new Set<Vendor>(vendorOptions.map(({ value }) => value));

const typeToGroupKey = Object.freeze(
  Object.fromEntries(
    Object.values(typeOptionsByVendor).flatMap((options) =>
      options.map(({ value, groupKey }) => [value, groupKey] as const),
    ),
  ),
) as Readonly<Partial<Record<LibraryTypeValue, LibraryGroupKey>>>;

const libraryIdToGroupKeyMap: Readonly<Record<string, LibraryGroupKey | undefined>> = {
  nvngx_dlss: 'dlss',
  nvngx_dlssg: 'dlss_g',
  nvngx_dlssd: 'dlss_d',
  amd_fidelityfx_dx12: 'fsr_31_dx12',
  amd_fidelityfx_vk: 'fsr_31_vk',
  amd_fidelityfx_loader_dx12: 'fsr_loader_dx12',
  amd_fidelityfx_upscaler_dx12: 'fsr_upscaler_dx12',
  amd_fidelityfx_framegeneration_dx12: 'fsr_framegeneration_dx12',
  amd_fidelityfx_denoiser_dx12: 'fsr_denoiser_dx12',
  amd_fidelityfx_radiancecache_dx12: 'fsr_radiancecache_dx12',
  libxess: 'xess',
  libxess_dx11: 'xess_dx11',
  libxess_fg: 'xess_fg',
  libxell: 'xell',
};

function isValidDate(date: Date): boolean {
  return !Number.isNaN(date.getTime());
}

export function groupKeyForType(typeValue: LibraryTypeValue): LibraryGroupKey {
  return typeToGroupKey[typeValue] ?? DEFAULT_GROUP_KEY;
}

export function libraryIdToGroupKey(libraryId: string): LibraryGroupKey {
  if (libraryId.startsWith('sl_')) {
    return 'streamline';
  }
  return libraryIdToGroupKeyMap[libraryId] ?? DEFAULT_GROUP_KEY;
}

export function getDefaultTypeForVendor(vendor: Vendor): LibraryTypeValue {
  return typeOptionsByVendor[vendor][0].value;
}

export function getTypeOptionsForVendor(vendor: Vendor): readonly LibraryTypeOption[] {
  return typeOptionsByVendor[vendor];
}

export function isVendor(value: unknown): value is Vendor {
  return typeof value === 'string' && vendorValues.has(value as Vendor);
}

export function formatVersionLabel(entry: LibraryManifestEntry): string {
  const version = trimToEmpty(entry.version.value);
  const label = trimToEmpty(entry.build.label);

  if (label) {
    return `${version || '—'} (${label})`;
  }

  return version || '—';
}

export function formatSignedDate(signature: LibraryManifestEntry['signature']): string {
  if (signature.status !== 'signed') {
    return t('libraries.unsigned');
  }

  const signedDate = new Date(signature.signed_at);

  if (!isValidDate(signedDate)) {
    return t('libraries.invalidDate');
  }

  return new Intl.DateTimeFormat(getLocale(), {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    timeZone: 'UTC',
  }).format(signedDate);
}
