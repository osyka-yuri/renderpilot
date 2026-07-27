import type { LibraryPackageSummary } from '@entities/library';

const PREVIEW_LIBRARY_PACKAGES = [
  {
    package_id: 'preview:nvidia:dlss:3.10.0',
    vendor: 'nvidia',
    technology: 'dlss_super_resolution',
    variant: 'runtime',
    display_name: 'NVIDIA DLSS Super Resolution',
    release: { version: '3.10.0', channel: 'stable', label: null },
    target: { os: 'windows', architecture: 'X64' },
    primary_file_name: 'nvngx_dlss.dll',
    primary_sha256: '2'.repeat(64),
    primary_signature: { status: 'signed', subject: 'NVIDIA Corporation', signed_at: null },
    legal_documents: [],
    size_bytes: 19_000_000,
    availability: 'available',
    local_state: 'absent',
    automatic_selection_allowed: true,
  },
  {
    package_id: 'preview:nvidia:dlss-fg:3.8.0',
    vendor: 'nvidia',
    technology: 'dlss_frame_generation',
    variant: 'runtime',
    display_name: 'NVIDIA DLSS Frame Generation',
    release: { version: '3.8.0', channel: 'stable', label: null },
    target: { os: 'windows', architecture: 'X64' },
    primary_file_name: 'nvngx_dlssg.dll',
    primary_sha256: '4'.repeat(64),
    primary_signature: { status: 'signed', subject: 'NVIDIA Corporation', signed_at: null },
    legal_documents: [],
    size_bytes: 24_000_000,
    availability: 'available',
    local_state: 'absent',
    automatic_selection_allowed: true,
  },
  {
    package_id: 'preview:intel:xess:2.0.1',
    vendor: 'intel',
    technology: 'intel_xess',
    variant: 'dx12_runtime',
    display_name: 'Intel XeSS',
    release: { version: '2.0.1', channel: 'stable', label: null },
    target: { os: 'windows', architecture: 'X64' },
    primary_file_name: 'libxess.dll',
    primary_sha256: '6'.repeat(64),
    primary_signature: { status: 'unsigned' },
    legal_documents: [],
    size_bytes: 14_000_000,
    availability: 'available',
    local_state: 'verified',
    automatic_selection_allowed: true,
  },
] as const satisfies readonly LibraryPackageSummary[];

/** Returns a fresh catalog snapshot so preview mutations never alter the seed. */
export function createMockLibraryPackages(): LibraryPackageSummary[] {
  return PREVIEW_LIBRARY_PACKAGES.map((item) => ({
    ...item,
    release: { ...item.release },
    target: { ...item.target },
    primary_signature: { ...item.primary_signature },
    legal_documents: [...item.legal_documents],
  }));
}
