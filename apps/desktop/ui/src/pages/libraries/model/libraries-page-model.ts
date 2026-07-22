import { trimToEmpty } from '@shared/text';
import { t, getLocale } from '@shared/i18n';
import type { LibraryPackageSummary, Signature } from '@entities/library';
import {
  libraryVendorOrder,
  vendorLabelForLibraryVendorKey,
  type LibraryVendorKey,
} from '@shared/graphics';

export type Vendor = Exclude<LibraryVendorKey, 'other'>;

export type VendorOption = Readonly<{
  value: Vendor;
  label: string;
}>;

export type LibraryTypeOption = Readonly<{
  value: string;
  label: string;
  technology: string;
  variants?: readonly string[];
}>;

export type LibraryTypeValue = (typeof typeOptionsByVendor)[Vendor][number]['value'];

/** The backend has already resolved and validated every package member. */
export type LibraryPackageRow = LibraryPackageSummary;

export const vendorOptions = libraryVendorOrder
  .filter((key): key is Vendor => key !== 'other')
  .map((value) => ({
    value,
    label: vendorLabelForLibraryVendorKey(value),
  })) satisfies readonly VendorOption[];

export const typeOptionsByVendor = {
  nvidia: [
    { value: 'dlss', label: 'DLSS', technology: 'dlss_super_resolution' },
    { value: 'dlss_fg', label: 'DLSS FG', technology: 'dlss_frame_generation' },
    { value: 'dlss_rr', label: 'DLSS RR', technology: 'dlss_ray_reconstruction' },
    { value: 'streamline', label: 'Streamline', technology: 'nvidia_streamline' },
  ],
  amd: [
    {
      value: 'fsr',
      label: 'FSR DX12',
      technology: 'amd_fsr',
      variants: ['dx12_runtime', 'sdk_bundle'],
    },
    {
      value: 'fsr_vk',
      label: 'FSR VK',
      technology: 'amd_fsr',
      variants: ['vulkan_runtime'],
    },
    { value: 'fsr_loader', label: 'FSR Loader', technology: 'amd_fsr_loader' },
    { value: 'fsr_upscaler', label: 'FSR Upscaler', technology: 'amd_fsr_upscaler' },
    {
      value: 'fsr_framegen',
      label: 'FSR FrameGen',
      technology: 'amd_fsr_frame_generation',
    },
    {
      value: 'fsr_denoiser',
      label: 'FSR Denoiser',
      technology: 'amd_fsr_ray_regeneration',
    },
    {
      value: 'fsr_radiancecache',
      label: 'FSR RadianceCache',
      technology: 'amd_fsr_radiance_cache',
    },
  ],
  intel: [
    {
      value: 'xess',
      label: 'XeSS',
      technology: 'intel_xess',
      variants: ['dx12_runtime'],
    },
    {
      value: 'xess_dx11',
      label: 'XeSS DX11',
      technology: 'intel_xess',
      variants: ['dx11_runtime'],
    },
    { value: 'xefg', label: 'XeFG', technology: 'intel_xefg' },
    { value: 'xell', label: 'XeLL', technology: 'intel_xell' },
  ],
  microsoft: [
    { value: 'dstorage', label: 'DirectStorage', technology: 'direct_storage' },
    { value: 'dxc', label: 'DXC', technology: 'microsoft_dxc' },
    { value: 'd3d12_agility', label: 'D3D12 Agility', technology: 'd3d12_agility' },
  ],
  valve: [{ value: 'openvr', label: 'OpenVR', technology: 'openvr' }],
} as const satisfies Record<Vendor, readonly LibraryTypeOption[]>;

const vendorValues = new Set<Vendor>(vendorOptions.map(({ value }) => value));

export function filterPackageRows(
  rows: readonly LibraryPackageRow[],
  vendor: Vendor,
  type: LibraryTypeValue,
): LibraryPackageRow[] {
  const options: readonly LibraryTypeOption[] = typeOptionsByVendor[vendor];
  const option = options.find((candidate) => candidate.value === type);
  if (!option) {
    return [];
  }
  return rows.filter(
    (row) =>
      row.vendor === vendor &&
      row.technology === option.technology &&
      (option.variants === undefined || option.variants.includes(row.variant)),
  );
}

export function findPackageRow(
  rows: readonly LibraryPackageRow[],
  packageId: string | null,
): LibraryPackageRow | null {
  if (packageId === null) {
    return null;
  }
  return rows.find((row) => row.package_id === packageId) ?? null;
}

/**
 * A package name only earns a separate line when the active list contains
 * genuinely different package families. The catalog remains fully descriptive;
 * this is strictly a contextual presentation decision.
 */
export function shouldShowPackageDisplayName(rows: readonly LibraryPackageRow[]): boolean {
  const names = new Set(rows.map((row) => normalizeDisplayName(row.display_name)));
  return names.size > 1;
}

/** Picks the newest stable package for every explicit technology/variant/target. */
export function selectLatestStablePackages(
  rows: readonly LibraryPackageRow[],
): LibraryPackageRow[] {
  const latest = new Map<string, LibraryPackageRow>();
  for (const row of rows) {
    if (row.release.channel !== 'stable') {
      continue;
    }
    const identity = [
      row.vendor,
      row.technology,
      row.variant,
      row.target.os,
      row.target.architecture,
      runtimeCompatibilityKey(row),
    ].join('\0');
    const current = latest.get(identity);
    if (!current || compareReleaseVersions(row.release.version, current.release.version) > 0) {
      latest.set(identity, row);
    }
  }
  return [...latest.values()];
}

function runtimeCompatibilityKey(row: LibraryPackageRow): string {
  const compatibility = row.target.compatibility;
  return compatibility ? `${compatibility.kind}:${compatibility.version}` : '';
}

/**
 * Orders dotted numeric versions without losing u64 precision. Non-numeric
 * versions fall back to locale-independent string ordering.
 */
export function compareReleaseVersions(left: string, right: string): number {
  const leftVersion = parseNumericVersion(left);
  const rightVersion = parseNumericVersion(right);
  if (leftVersion && rightVersion) {
    const length = Math.max(leftVersion.length, rightVersion.length);
    for (let index = 0; index < length; index += 1) {
      const leftSegment = leftVersion[index] ?? 0n;
      const rightSegment = rightVersion[index] ?? 0n;
      if (leftSegment !== rightSegment) {
        return leftSegment < rightSegment ? -1 : 1;
      }
    }
    return 0;
  }
  return left.localeCompare(right, 'en');
}

function parseNumericVersion(value: string): bigint[] | null {
  if (!/^\d+(?:\.\d+)*$/.test(value)) {
    return null;
  }
  try {
    return value.split('.').map(BigInt);
  } catch {
    return null;
  }
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

export function formatVersionLabel(row: LibraryPackageRow): string {
  const version = trimToEmpty(row.release.version);
  const label = trimToEmpty(row.release.label);
  return label ? `${version || '—'} (${label})` : version || '—';
}

function normalizeDisplayName(value: string): string {
  return trimToEmpty(value).replace(/\s+/gu, ' ').toLowerCase();
}

export function formatArchitectureLabel(row: LibraryPackageRow): string {
  return row.target.architecture === 'X86' ? 'x86' : 'x64';
}

export function formatSignedDate(signature: Signature): string {
  if (signature.status !== 'signed') {
    return t('libraries.unsigned');
  }
  if (signature.signed_at === null) {
    return '—';
  }
  const signedDate = new Date(signature.signed_at);
  if (Number.isNaN(signedDate.getTime())) {
    return t('libraries.invalidDate');
  }
  return new Intl.DateTimeFormat(getLocale(), {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    timeZone: 'UTC',
  }).format(signedDate);
}
