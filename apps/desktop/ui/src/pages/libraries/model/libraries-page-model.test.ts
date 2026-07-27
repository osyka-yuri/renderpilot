import { describe, expect, it } from 'vitest';

import { t } from '@shared/i18n';
import {
  compareReleaseVersions,
  findPackageRow,
  filterPackageRows,
  formatArchitectureLabel,
  formatSignedDate,
  formatVersionLabel,
  selectLatestStablePackages,
  shouldDeleteLibraryPackage,
  shouldShowPackageDisplayName,
  typeOptionsByVendor,
} from './libraries-page-model';
import { legalDocumentLink, packagesOf } from './library-package-test-fixtures';

describe('library package presentation', () => {
  it('resolves a selected package against the latest snapshot', () => {
    const initial = packagesOf([
      { id: 'openvr', vendor: 'valve', technology: 'openvr', legalDocuments: [] },
    ]);
    const refreshed = packagesOf([
      {
        id: 'openvr',
        vendor: 'valve',
        technology: 'openvr',
        legalDocuments: [legalDocumentLink()],
      },
    ]);

    expect(findPackageRow(initial, 'openvr')?.legal_documents).toEqual([]);
    expect(findPackageRow(refreshed, 'openvr')?.legal_documents).toEqual([legalDocumentLink()]);
    expect(findPackageRow(refreshed, 'missing')).toBeNull();
  });

  it('exposes OpenVR as Valve catalog data without UI-side inference', () => {
    expect(typeOptionsByVendor.valve).toEqual([
      { value: 'openvr', label: 'OpenVR', technology: 'openvr' },
    ]);
    const rows = packagesOf([
      { id: 'openvr.x64', vendor: 'valve', technology: 'openvr', variant: 'runtime' },
      { id: 'other', vendor: 'nvidia', technology: 'nvidia_dlss_sr', variant: 'runtime' },
    ]);
    expect(filterPackageRows(rows, 'valve', 'openvr').map((row) => row.package_id)).toEqual([
      'openvr.x64',
    ]);
  });

  it('filters backend summaries by vendor, technology, and explicit variant', () => {
    const rows = packagesOf([
      { id: 'fsr.dx12', vendor: 'amd', technology: 'amd_fsr', variant: 'dx12_runtime' },
      { id: 'fsr.sdk', vendor: 'amd', technology: 'amd_fsr', variant: 'sdk_bundle' },
      { id: 'fsr.vulkan', vendor: 'amd', technology: 'amd_fsr', variant: 'vulkan_runtime' },
    ]);

    expect(filterPackageRows(rows, 'amd', 'fsr').map((row) => row.package_id)).toEqual([
      'fsr.dx12',
      'fsr.sdk',
    ]);
    expect(filterPackageRows(rows, 'amd', 'fsr_vk').map((row) => row.package_id)).toEqual([
      'fsr.vulkan',
    ]);
  });

  it('uses numeric version ordering for both latest selection and table sorting', () => {
    const rows = packagesOf([
      {
        id: 'dx12.old',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '4.0.9',
      },
      {
        id: 'dx12.new',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '4.0.10',
      },
      {
        id: 'debug.newer',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '9.0.0',
        channel: 'debug',
      },
    ]);

    expect(selectLatestStablePackages(rows).map((row) => row.package_id)).toEqual(['dx12.new']);
    expect(compareReleaseVersions('4.0.10', '4.0.9')).toBeGreaterThan(0);
  });

  it('never selects a withdrawn local-only stable package for bulk download', () => {
    const rows = packagesOf([
      {
        id: 'dxc.available',
        vendor: 'microsoft',
        technology: 'dxc',
        version: '1.9.0',
        availability: 'available',
      },
      {
        id: 'dxc.withdrawn',
        vendor: 'microsoft',
        technology: 'dxc',
        version: '2.0.0',
        availability: 'local_only',
      },
    ]);

    expect(selectLatestStablePackages(rows).map((row) => row.package_id)).toEqual([
      'dxc.available',
    ]);
  });

  it('keeps a missing withdrawn package removable from Libraries', () => {
    const [withdrawn, active] = packagesOf([
      { id: 'withdrawn', availability: 'local_only', isDownloaded: false },
      { id: 'active', availability: 'available', isDownloaded: false },
    ]);

    expect(shouldDeleteLibraryPackage(withdrawn)).toBe(true);
    expect(shouldDeleteLibraryPackage(active)).toBe(false);
  });

  it('repairs active damaged packages by download and keeps withdrawn ones removable', () => {
    const [withdrawnCorrupt, activeCorrupt] = packagesOf([
      {
        id: 'withdrawn-corrupt',
        availability: 'local_only',
        localState: 'corrupt',
      },
      {
        id: 'active-corrupt',
        availability: 'available',
        localState: 'corrupt',
      },
    ]);

    expect(shouldDeleteLibraryPackage(withdrawnCorrupt)).toBe(true);
    expect(shouldDeleteLibraryPackage(activeCorrupt)).toBe(false);
    expect(selectLatestStablePackages([withdrawnCorrupt, activeCorrupt])).toEqual([activeCorrupt]);
  });

  it('keeps distinct runtime compatibility targets in the latest selection', () => {
    const selected = selectLatestStablePackages(
      packagesOf([
        {
          id: 'agility.617.old',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.617.0',
          compatibilityVersion: 617,
        },
        {
          id: 'agility.617.new',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.617.1',
          compatibilityVersion: 617,
        },
        {
          id: 'agility.618',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.618.0',
          compatibilityVersion: 618,
        },
      ]),
    );

    expect(selected.map((row) => row.package_id).sort()).toEqual([
      'agility.617.new',
      'agility.618',
    ]);
  });

  it('orders every version segment with u64 precision', () => {
    expect(
      compareReleaseVersions('1.18446744073709551615', '1.18446744073709551614'),
    ).toBeGreaterThan(0);
  });

  it('orders Microsoft prereleases with NuGet precedence', () => {
    expect(
      compareReleaseVersions('1.4.0-preview2-2606.904', '1.4.0-preview1-2603.504'),
    ).toBeGreaterThan(0);
    expect(compareReleaseVersions('1.721.2', '1.721.2-preview')).toBeGreaterThan(0);
    expect(
      compareReleaseVersions(
        '1.0.0-preview.18446744073709551615',
        '1.0.0-preview.9999999999999999999',
      ),
    ).toBeGreaterThan(0);
  });

  it('formats release, target, and primary signature from the compact summary', () => {
    const [row] = packagesOf([
      {
        id: 'dxc.1.9',
        version: '1.9.0',
        label: 'SDK release',
        architecture: 'X86',
        signature: { status: 'signed', signed_at: null },
      },
    ]);

    expect(formatVersionLabel(row)).toBe('1.9.0 (SDK release)');
    expect(formatArchitectureLabel(row)).toBe('x86');
    expect(formatSignedDate(row.primary_signature)).toBe('—');
    expect(formatSignedDate({ status: 'unsigned' })).toBe(t('libraries.unsigned'));
    expect(formatSignedDate({ status: 'signed', signed_at: 'invalid' })).toBe(
      t('libraries.invalidDate'),
    );
  });

  it('appends the supplemental catalog annotation verbatim', () => {
    const [legacyRuntime, sdkBundle, preview, beta] = packagesOf([
      {
        id: 'fsr.legacy',
        version: '1.0.1.41314',
        label: 'FSR 3.1.4',
      },
      {
        id: 'fsr.sdk',
        version: '4.1.1.2740',
        label: null,
      },
      {
        id: 'radiance.preview',
        version: '0.9.0.2740',
        label: 'preview',
      },
      {
        id: 'sdk.beta',
        version: '4.1.1.2740',
        label: 'Beta White Collie',
        channel: 'beta',
      },
    ]);

    expect(formatVersionLabel(legacyRuntime)).toBe('1.0.1.41314 (FSR 3.1.4)');
    expect(formatVersionLabel(sdkBundle)).toBe('4.1.1.2740');
    expect(formatVersionLabel(preview)).toBe('0.9.0.2740 (preview)');
    expect(formatVersionLabel(beta)).toBe('4.1.1.2740 (Beta White Collie)');
  });

  it('shows package names only when the active list contains distinct names', () => {
    expect(
      shouldShowPackageDisplayName(
        packagesOf([
          { id: 'dlss.old', displayName: 'NVIDIA DLSS Super Resolution' },
          { id: 'dlss.new', displayName: '  nvidia dlss   super resolution ' },
        ]),
      ),
    ).toBe(false);
    expect(
      shouldShowPackageDisplayName(
        packagesOf([
          { id: 'fsr.legacy', displayName: 'AMD FidelityFX Super Resolution' },
          { id: 'fsr.sdk', displayName: 'AMD FidelityFX SDK DirectX 12' },
        ]),
      ),
    ).toBe(true);
  });
});
